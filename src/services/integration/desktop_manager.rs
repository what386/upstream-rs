#[cfg(target_os = "linux")]
use crate::services::artifact::AppImageExtractor;
use crate::{
    models::{
        common::{DesktopEntry, enums::Filetype},
        upstream::Package,
    },
    utils::{
        filesystem::{atomic_ops::write_atomic, path_exists_no_follow},
        static_paths::UpstreamPaths,
    },
};
use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::IconManager;

#[cfg(windows)]
use std::process::Command;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

#[derive(Debug, Clone)]
enum FileSnapshot {
    Missing(PathBuf),
    File { path: PathBuf, contents: Vec<u8> },
    Symlink { path: PathBuf, target: PathBuf },
}

impl FileSnapshot {
    fn capture(path: PathBuf) -> Result<Self> {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::Missing(path));
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to inspect '{}'", path.display()));
            }
        };

        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)
                .with_context(|| format!("Failed to snapshot symlink '{}'", path.display()))?;
            return Ok(Self::Symlink { path, target });
        }
        if metadata.is_file() {
            let contents = fs::read(&path)
                .with_context(|| format!("Failed to snapshot file '{}'", path.display()))?;
            return Ok(Self::File { path, contents });
        }

        Err(anyhow!(
            "Cannot snapshot unsupported desktop integration path '{}'",
            path.display()
        ))
    }

    fn path(&self) -> &Path {
        match self {
            Self::Missing(path) | Self::File { path, .. } | Self::Symlink { path, .. } => path,
        }
    }

    fn restore(&self) -> Result<()> {
        let path = self.path();
        if path_exists_no_follow(path)? {
            fs::remove_file(path).with_context(|| {
                format!(
                    "Failed to remove replacement desktop integration '{}'",
                    path.display()
                )
            })?;
        }

        match self {
            Self::Missing(_) => Ok(()),
            Self::File { path, contents } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!(
                            "Failed to create desktop integration directory '{}'",
                            parent.display()
                        )
                    })?;
                }
                write_atomic(path, contents).with_context(|| {
                    format!(
                        "Failed to restore desktop integration file '{}'",
                        path.display()
                    )
                })
            }
            Self::Symlink { path, target } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!(
                            "Failed to create desktop integration directory '{}'",
                            parent.display()
                        )
                    })?;
                }
                create_file_symlink(target, path).with_context(|| {
                    format!(
                        "Failed to restore desktop integration symlink '{}'",
                        path.display()
                    )
                })
            }
        }
    }
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[derive(Debug, Clone)]
pub struct DesktopSnapshot {
    entry: FileSnapshot,
    icon: Option<FileSnapshot>,
}

impl DesktopSnapshot {
    pub fn restore(&self, replacement: Option<&Package>) -> Result<()> {
        if let Some(replacement_icon) = replacement.and_then(|package| package.icon_path.as_ref())
            && self
                .icon
                .as_ref()
                .is_none_or(|snapshot| snapshot.path() != replacement_icon)
            && path_exists_no_follow(replacement_icon)?
        {
            fs::remove_file(replacement_icon).with_context(|| {
                format!(
                    "Failed to remove replacement icon '{}'",
                    replacement_icon.display()
                )
            })?;
        }

        if let Some(icon) = &self.icon {
            icon.restore()?;
        }
        self.entry.restore()
    }
}

pub struct DesktopManager<'a> {
    paths: &'a UpstreamPaths,
    #[cfg(target_os = "linux")]
    extractor: &'a AppImageExtractor,
}

impl<'a> DesktopManager<'a> {
    #[cfg(target_os = "linux")]
    pub fn new(paths: &'a UpstreamPaths, extractor: &'a AppImageExtractor) -> Self {
        Self { paths, extractor }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub async fn enable_package_entry<H>(
        &self,
        package: &mut Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = package
            .install_path
            .clone()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        #[cfg(target_os = "linux")]
        let icon_manager = IconManager::new(self.paths, self.extractor);
        #[cfg(not(target_os = "linux"))]
        let icon_manager = IconManager::new(self.paths);

        let previous_icon_path = package.icon_path.clone();
        let icon_path = icon_manager
            .add_icon(
                &package.name,
                &install_path,
                &package.filetype,
                message_callback,
            )
            .await
            .context(format!("Failed to add icon for '{}'", package.name))?;

        let mut desktop_package = package.clone();
        desktop_package.icon_path = icon_path.clone();
        let desktop_entry = DesktopEntry::from_package(&desktop_package);

        if let Err(err) = self
            .create_entry(
                &install_path,
                &desktop_package.filetype,
                desktop_entry,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to create desktop entry for '{}'",
                desktop_package.name
            ))
        {
            if let Some(new_icon_path) = icon_path.as_ref()
                && Some(new_icon_path) != previous_icon_path.as_ref()
                && new_icon_path.exists()
            {
                fs::remove_file(new_icon_path).context(format!(
                    "Failed to remove icon file at '{}'",
                    new_icon_path.display()
                ))?;
            }
            return Err(err);
        }

        package.icon_path = icon_path;

        if let Some(previous_icon_path) = previous_icon_path
            && Some(&previous_icon_path) != package.icon_path.as_ref()
            && previous_icon_path.exists()
        {
            fs::remove_file(&previous_icon_path).context(format!(
                "Failed to remove previous icon file at '{}'",
                previous_icon_path.display()
            ))?;
        }

        Ok(())
    }

    pub fn disable_package_entry<H>(
        &self,
        package: &mut Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Removing desktop entry ...");
        Self::remove_entry(self.paths, &package.name).context(format!(
            "Failed to remove desktop entry for '{}'",
            package.name
        ))?;

        if let Some(icon_path) = package.icon_path.take()
            && icon_path.exists()
        {
            fs::remove_file(&icon_path).context(format!(
                "Failed to remove icon file at '{}'",
                icon_path.display()
            ))?;
            message!(
                message_callback,
                "Removed stored icon: {}",
                icon_path.display()
            );
        }

        Ok(())
    }

    pub async fn create_entry<H>(
        &self,
        install_path: &Path,
        filetype: &Filetype,
        entry: DesktopEntry,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        #[cfg(target_os = "linux")]
        {
            return self
                .create_unix_desktop_entry(install_path, filetype, entry, message_callback)
                .await;
        }

        #[cfg(target_os = "macos")]
        {
            let name = entry
                .name
                .as_deref()
                .ok_or_else(|| anyhow!("Desktop entry name is required"))?;
            let _ = (&entry.icon, &entry.comment, &entry.categories);
            let exec_path = entry
                .exec
                .as_deref()
                .map(Path::new)
                .ok_or_else(|| anyhow!("Desktop entry exec path is required"))?;

            return self.create_macos_launcher(
                name,
                install_path,
                exec_path,
                filetype,
                message_callback,
            );
        }

        #[cfg(windows)]
        {
            let name = entry
                .name
                .as_deref()
                .ok_or_else(|| anyhow!("Desktop entry name is required"))?;
            let _ = (install_path, filetype, message_callback);
            let exec_path = entry
                .exec
                .as_deref()
                .map(Path::new)
                .ok_or_else(|| anyhow!("Desktop entry exec path is required"))?;
            let icon_path = entry
                .icon
                .as_deref()
                .filter(|icon| !icon.is_empty())
                .map(Path::new);

            return self.create_windows_shortcut(name, exec_path, icon_path);
        }
    }

    pub fn remove_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        let path = Self::managed_entry_path(paths, name);

        #[cfg(target_os = "linux")]
        {
            if path.exists() {
                fs::remove_file(&path)?;
            }
            Ok(())
        }

        #[cfg(target_os = "macos")]
        {
            if path.exists() {
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.file_type().is_symlink() {
                    fs::remove_file(&path)?;
                }
            }
            return Ok(());
        }

        #[cfg(windows)]
        {
            if path.exists() {
                fs::remove_file(&path)?;
            }
            return Ok(());
        }
    }

    pub fn snapshot_for_package(
        paths: &UpstreamPaths,
        package: &Package,
    ) -> Result<DesktopSnapshot> {
        let entry = FileSnapshot::capture(Self::managed_entry_path(paths, &package.name))?;
        let icon = package
            .icon_path
            .as_ref()
            .map(|path| FileSnapshot::capture(path.clone()))
            .transpose()?;
        Ok(DesktopSnapshot { entry, icon })
    }

    pub fn rename_entry(paths: &UpstreamPaths, old_name: &str, new_name: &str) -> Result<bool> {
        let source = Self::managed_entry_path(paths, old_name);
        let destination = Self::managed_entry_path(paths, new_name);

        if path_exists_no_follow(&destination)? {
            return Err(anyhow!(
                "Refusing to overwrite existing desktop entry '{}'",
                destination.display()
            ));
        }
        if !path_exists_no_follow(&source)? {
            return Ok(false);
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create desktop entry directory '{}'",
                    parent.display()
                )
            })?;
        }
        fs::rename(&source, &destination).with_context(|| {
            format!(
                "Failed to rename desktop entry '{}' to '{}'",
                source.display(),
                destination.display()
            )
        })?;
        Ok(true)
    }

    fn managed_entry_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        #[cfg(target_os = "linux")]
        {
            paths
                .integration
                .xdg_applications_dir
                .join(format!("{name}.desktop"))
        }

        #[cfg(target_os = "macos")]
        {
            Self::macos_launcher_path(paths, name)
        }

        #[cfg(windows)]
        {
            Self::windows_shortcut_path(paths, name)
        }
    }

    #[cfg(target_os = "linux")]
    fn merge_embedded_entry(
        embedded: DesktopEntry,
        generated: DesktopEntry,
        fallback_name: &str,
    ) -> DesktopEntry {
        let embedded_name = embedded
            .name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned);
        let mut merged = embedded.merge(generated).ensure_name(fallback_name);
        if embedded_name.is_some() {
            merged.name = embedded_name;
        }
        merged
    }

    /// Build and write a Linux desktop entry.
    ///
    /// For AppImages, this attempts to merge metadata from an embedded
    /// `.desktop` file before applying explicit entry overrides.
    #[cfg(target_os = "linux")]
    async fn create_unix_desktop_entry<H>(
        &self,
        install_path: &Path,
        filetype: &Filetype,
        mut entry: DesktopEntry,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        let name = entry
            .name
            .as_deref()
            .ok_or_else(|| anyhow!("Desktop entry name is required"))?
            .to_string();

        entry = if *filetype == Filetype::AppImage {
            let squashfs_root = self
                .extractor
                .extract(&name, install_path, message_callback)
                .await?;
            let embedded = self
                .find_and_parse_desktop_file(&squashfs_root, &name, message_callback)
                .unwrap_or_default();
            Self::merge_embedded_entry(embedded, entry, &name)
        } else {
            entry.ensure_name(&name)
        };

        entry.terminal = false;
        self.write_unix_entry(&name, &entry)
    }

    #[cfg(target_os = "linux")]
    fn write_unix_entry(&self, name: &str, entry: &DesktopEntry) -> Result<PathBuf> {
        let out_path = self
            .paths
            .integration
            .xdg_applications_dir
            .join(format!("{}.desktop", name));
        write_atomic(&out_path, entry.to_desktop_file().as_bytes())?;
        Ok(out_path)
    }

    /// Search common and fallback locations in extracted AppImage contents for
    /// the most relevant `.desktop` file.
    #[cfg(target_os = "linux")]
    fn find_and_parse_desktop_file<H>(
        &self,
        squashfs_root: &Path,
        name: &str,
        message_callback: &mut Option<H>,
    ) -> Option<DesktopEntry>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Searching for embedded .desktop file ...");

        let candidates = [
            squashfs_root.join(format!("{}.desktop", name)),
            squashfs_root.join(format!("usr/share/applications/{}.desktop", name)),
        ];

        for path in &candidates {
            if path.exists() {
                message!(message_callback, "Found .desktop file: {}", path.display());
                return Self::parse_desktop_file(path);
            }
        }

        let pattern = format!("{}/**/*.desktop", squashfs_root.display());
        if let Ok(entries) = glob::glob(&pattern) {
            let mut found: Vec<PathBuf> = entries.flatten().collect();
            found.sort_by_key(|p| {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if stem.eq_ignore_ascii_case(name) {
                    0
                } else {
                    1
                }
            });

            if let Some(path) = found.first() {
                message!(message_callback, "Found .desktop file: {}", path.display());
                return Self::parse_desktop_file(path);
            }
        }

        message!(message_callback, "No .desktop file found in AppImage");
        None
    }

    /// Parse a `.desktop` file and extract only the `[Desktop Entry]` section.
    #[cfg(target_os = "linux")]
    fn parse_desktop_file(path: &Path) -> Option<DesktopEntry> {
        let content = fs::read_to_string(path).ok()?;
        let mut entry = DesktopEntry::default();
        let mut in_desktop_entry = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_desktop_entry = trimmed.eq_ignore_ascii_case("[Desktop Entry]");
                continue;
            }

            if !in_desktop_entry
                || trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with(';')
                || !trimmed.contains('=')
            {
                continue;
            }
            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };
            let key = key.trim().trim_start_matches('\u{feff}');
            let value = value.trim().to_string();
            entry.set_field(key, value);
        }

        Some(entry)
    }

    #[cfg(target_os = "macos")]
    fn macos_launcher_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        let apps_dir = dirs::home_dir()
            .unwrap_or_else(|| paths.dirs.user_dir.clone())
            .join("Applications");
        apps_dir.join(format!("{name}.app"))
    }

    /// Resolve the `.app` bundle corresponding to an installed macOS package.
    #[cfg(target_os = "macos")]
    fn find_app_bundle_path(
        install_path: &Path,
        exec_path: &Path,
        filetype: &Filetype,
    ) -> Option<PathBuf> {
        use std::ffi::OsStr;

        if matches!(filetype, Filetype::MacApp)
            && install_path.extension() == Some(OsStr::new("app"))
        {
            return Some(install_path.to_path_buf());
        }

        if install_path.extension() == Some(OsStr::new("app")) {
            return Some(install_path.to_path_buf());
        }

        for candidate in [exec_path, install_path] {
            if let Some(bundle) = candidate
                .ancestors()
                .find(|ancestor| ancestor.extension() == Some(OsStr::new("app")))
            {
                return Some(bundle.to_path_buf());
            }
        }

        None
    }

    #[cfg(target_os = "macos")]
    fn create_macos_launcher<H>(
        &self,
        name: &str,
        install_path: &Path,
        exec_path: &Path,
        filetype: &Filetype,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        let app_bundle = Self::find_app_bundle_path(install_path, exec_path, filetype)
            .ok_or_else(|| anyhow!("Could not locate a .app bundle for '{}'", name))?;

        if !app_bundle.exists() || !app_bundle.is_dir() {
            return Err(anyhow!(
                "Resolved .app bundle '{}' does not exist or is not a directory",
                app_bundle.display()
            ));
        }

        let launcher_path = Self::macos_launcher_path(self.paths, name);
        if let Some(parent) = launcher_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if launcher_path.exists() {
            let metadata = fs::symlink_metadata(&launcher_path)?;
            if metadata.file_type().is_symlink() {
                fs::remove_file(&launcher_path)?;
            } else {
                return Err(anyhow!(
                    "Refusing to overwrite non-symlink at '{}'",
                    launcher_path.display()
                ));
            }
        }

        std::os::unix::fs::symlink(&app_bundle, &launcher_path)?;
        message!(
            message_callback,
            "Created macOS launcher: {} -> {}",
            launcher_path.display(),
            app_bundle.display()
        );

        Ok(launcher_path)
    }

    #[cfg(windows)]
    fn windows_shortcut_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        let shortcut_dir =
            dirs::desktop_dir().unwrap_or_else(|| paths.dirs.data_dir.join("shortcuts"));
        shortcut_dir.join(format!("{}.lnk", name))
    }

    #[cfg(windows)]
    fn ps_quote(value: &str) -> String {
        value.replace('\'', "''")
    }

    #[cfg(windows)]
    fn create_windows_shortcut(
        &self,
        name: &str,
        exec_path: &Path,
        icon_path: Option<&Path>,
    ) -> Result<PathBuf> {
        let shortcut_path = Self::windows_shortcut_path(self.paths, name);
        if let Some(parent) = shortcut_path.parent() {
            fs::create_dir_all(parent).context("Failed to create shortcut directory")?;
        }

        let target = Self::ps_quote(&exec_path.display().to_string());
        let shortcut = Self::ps_quote(&shortcut_path.display().to_string());
        let working_dir = exec_path
            .parent()
            .map(|p| Self::ps_quote(&p.display().to_string()))
            .unwrap_or_default();

        let mut script = vec![
            "$WshShell = New-Object -ComObject WScript.Shell".to_string(),
            format!("$Shortcut = $WshShell.CreateShortcut('{}')", shortcut),
            format!("$Shortcut.TargetPath = '{}'", target),
        ];

        if !working_dir.is_empty() {
            script.push(format!("$Shortcut.WorkingDirectory = '{}'", working_dir));
        }

        if let Some(icon) = icon_path {
            let icon_value = Self::ps_quote(&icon.display().to_string());
            script.push(format!("$Shortcut.IconLocation = '{},0'", icon_value));
        }

        script.push("$Shortcut.Save()".to_string());

        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script.join("; "),
            ])
            .status()
            .context("Failed to execute PowerShell for shortcut creation")?;

        if !status.success() {
            anyhow::bail!(
                "Failed to create Windows shortcut '{}' (PowerShell exit status: {})",
                shortcut_path.display(),
                status
            );
        }

        Ok(shortcut_path)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::DesktopManager;
    use crate::models::{
        common::{
            DesktopEntry,
            enums::{Channel, Filetype, Provider},
        },
        upstream::Package,
    };
    use crate::utils::test_support;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-desktop-manager-test-{name}-{nanos}"))
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn fixture_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(relative)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_desktop_file_reads_valid_fixture() {
        let desktop_file = fixture_path("integration/desktop/tool-valid.desktop");

        let entry = DesktopManager::parse_desktop_file(&desktop_file).expect("parse desktop file");

        assert_eq!(entry.name.as_deref(), Some("Tool"));
        assert_eq!(entry.exec.as_deref(), Some("/usr/bin/tool"));
        assert_eq!(entry.icon.as_deref(), Some("tool"));
    }

    #[test]
    fn parse_desktop_file_preserves_localized_and_extra_fields() {
        let root = temp_root("parse");
        fs::create_dir_all(&root).expect("create temp root");
        let desktop_file = root.join("app.desktop");

        fs::write(
            &desktop_file,
            include_str!("../../../tests/fixtures/integration/desktop/preserved-fields.desktop"),
        )
        .expect("write desktop file");

        let entry = DesktopManager::parse_desktop_file(&desktop_file).expect("parse desktop file");

        assert_eq!(entry.name.as_deref(), Some("KDE Connect"));
        assert_eq!(entry.comment.as_deref(), Some("Make all your devices one"));
        assert_eq!(entry.exec.as_deref(), Some("kdeconnect-app"));
        assert_eq!(entry.icon.as_deref(), Some("kdeconnect"));
        assert_eq!(entry.categories.as_deref(), Some("Qt;KDE;Network"));
        assert!(!entry.terminal);

        assert_eq!(
            entry.extras.get("Name[fr]").map(String::as_str),
            Some("KDEConnect")
        );
        assert_eq!(
            entry.extras.get("GenericName").map(String::as_str),
            Some("Device Synchronization")
        );
        assert_eq!(
            entry.extras.get("X-AppImage-Name").map(String::as_str),
            Some("KDE_Connect")
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn ensure_name_prefers_localized_then_fallback() {
        let mut localized_only = DesktopEntry::default();
        localized_only.set_field("Name[en_GB]", "Localized App".to_string());

        let localized_resolved = localized_only.ensure_name("fallback-name");
        assert_eq!(localized_resolved.name.as_deref(), Some("Localized App"));

        let fallback_resolved = DesktopEntry::default().ensure_name("fallback-name");
        assert_eq!(fallback_resolved.name.as_deref(), Some("fallback-name"));
    }

    #[test]
    fn serialize_preserves_extras_and_sanitize_overrides_exec_icon_terminal() {
        let mut entry = DesktopEntry::default();
        entry.set_field("Name[en_GB]", "Localized App".to_string());
        entry.set_field("X-AppImage-Version", "25.12.2-1".to_string());
        entry.set_field("Exec", "embedded-exec".to_string());
        entry.set_field("Icon", "embedded-icon".to_string());
        entry.set_field("Terminal", "true".to_string());

        let rendered = entry
            .ensure_name("fallback-name")
            .sanitize(Path::new("/tmp/upstream-bin"), None)
            .to_desktop_file();

        assert!(rendered.contains("Name=Localized App\n"));
        assert!(rendered.contains("Exec=/tmp/upstream-bin\n"));
        assert!(rendered.contains("Icon=\n"));
        assert!(rendered.contains("Terminal=false\n"));
        assert!(rendered.contains("Name[en_GB]=Localized App\n"));
        assert!(rendered.contains("X-AppImage-Version=25.12.2-1\n"));
    }

    #[cfg(unix)]
    #[test]
    fn rename_entry_moves_file_without_overwriting_existing_destination() {
        let root = temp_root("rename");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.integration.xdg_applications_dir)
            .expect("create applications dir");
        let old = paths.integration.xdg_applications_dir.join("old.desktop");
        let new = paths.integration.xdg_applications_dir.join("new.desktop");
        fs::write(&old, "old desktop\n").expect("write old desktop");

        assert!(DesktopManager::rename_entry(&paths, "old", "new").expect("rename entry"));
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(&new).expect("read new"), "old desktop\n");

        fs::write(&old, "another desktop\n").expect("write another old desktop");
        let error = DesktopManager::rename_entry(&paths, "old", "new")
            .expect_err("existing destination should be preserved");
        assert!(error.to_string().contains("Refusing to overwrite"));
        assert_eq!(
            fs::read_to_string(&old).expect("read preserved old"),
            "another desktop\n"
        );
        assert_eq!(
            fs::read_to_string(&new).expect("read preserved new"),
            "old desktop\n"
        );

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn desktop_snapshot_restores_entry_and_icon_after_replacement() {
        let root = temp_root("snapshot");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.integration.xdg_applications_dir)
            .expect("create applications dir");
        fs::create_dir_all(&paths.state.icons_dir).expect("create icons dir");
        let entry_path = paths.integration.xdg_applications_dir.join("tool.desktop");
        let old_icon_path = paths.state.icons_dir.join("tool-old.png");
        let new_icon_path = paths.state.icons_dir.join("tool-new.png");
        fs::write(&entry_path, b"old desktop").expect("write old desktop");
        fs::write(&old_icon_path, b"old icon").expect("write old icon");

        let mut previous = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        previous.icon_path = Some(old_icon_path.clone());
        let snapshot =
            DesktopManager::snapshot_for_package(&paths, &previous).expect("snapshot desktop");

        fs::write(&entry_path, b"new desktop").expect("write new desktop");
        fs::write(&old_icon_path, b"overwritten icon").expect("overwrite old icon");
        fs::write(&new_icon_path, b"new icon").expect("write new icon");
        let mut replacement = previous.clone();
        replacement.icon_path = Some(new_icon_path.clone());

        snapshot
            .restore(Some(&replacement))
            .expect("restore desktop snapshot");

        assert_eq!(fs::read(&entry_path).expect("read entry"), b"old desktop");
        assert_eq!(fs::read(&old_icon_path).expect("read icon"), b"old icon");
        assert!(!new_icon_path.exists());

        cleanup(&root).expect("cleanup");
    }
}
