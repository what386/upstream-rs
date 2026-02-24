use crate::{
    models::common::{DesktopEntry, enums::Filetype},
    services::integration::appimage_extractor::AppImageExtractor,
    utils::static_paths::UpstreamPaths,
};
#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
#[cfg(target_os = "macos")]
use anyhow::anyhow;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::process::Command;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct DesktopManager<'a> {
    paths: &'a UpstreamPaths,
    #[cfg(target_os = "linux")]
    extractor: &'a AppImageExtractor,
}

impl<'a> DesktopManager<'a> {
    pub fn new(paths: &'a UpstreamPaths, extractor: &'a AppImageExtractor) -> Self {
        Self {
            paths,
            #[cfg(target_os = "linux")]
            extractor,
        }
    }

    pub async fn create_entry<H>(
        &self,
        name: &str,
        install_path: &Path,
        exec_path: &Path,
        icon_path: Option<&Path>,
        filetype: &Filetype,
        comment: Option<&str>,
        categories: Option<&str>,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        #[cfg(target_os = "linux")]
        {
            return self
                .create_unix_desktop_entry(
                    name,
                    install_path,
                    exec_path,
                    icon_path,
                    filetype,
                    comment,
                    categories,
                    message_callback,
                )
                .await;
        }

        #[cfg(target_os = "macos")]
        {
            let _ = (icon_path, comment, categories);
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
            let _ = (
                install_path,
                filetype,
                comment,
                categories,
                message_callback,
            );
            return self.create_windows_shortcut(name, exec_path, icon_path);
        }
    }

    pub fn remove_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let path = paths
                .integration
                .xdg_applications_dir
                .join(format!("{}.desktop", name));
            if path.exists() {
                fs::remove_file(&path)?;
            }
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            let path = Self::macos_launcher_path(paths, name);
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
            let path = Self::windows_shortcut_path(paths, name);
            if path.exists() {
                fs::remove_file(&path)?;
            }
            return Ok(());
        }
    }

    #[cfg(target_os = "linux")]
    async fn create_unix_desktop_entry<H>(
        &self,
        name: &str,
        install_path: &Path,
        exec_path: &Path,
        icon_path: Option<&Path>,
        filetype: &Filetype,
        comment: Option<&str>,
        categories: Option<&str>,
        message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        let fallback_entry = DesktopEntry {
            comment: comment.map(String::from),
            categories: categories.map(String::from),
            ..DesktopEntry::default()
        };

        let entry = if *filetype == Filetype::AppImage {
            let squashfs_root = self
                .extractor
                .extract(name, install_path, message_callback)
                .await?;
            fallback_entry
                .merge(
                    self.find_and_parse_desktop_file(&squashfs_root, name, message_callback)
                        .unwrap_or_default(),
                )
                .ensure_name(name)
        } else {
            fallback_entry.ensure_name(name)
        };

        let entry = entry.sanitize(exec_path, icon_path);
        self.write_unix_entry(name, &entry)
    }

    #[cfg(target_os = "linux")]
    fn write_unix_entry(&self, name: &str, entry: &DesktopEntry) -> Result<PathBuf> {
        let out_path = self
            .paths
            .integration
            .xdg_applications_dir
            .join(format!("{}.desktop", name));
        fs::write(&out_path, entry.to_desktop_file())?;
        Ok(out_path)
    }

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
#[path = "../../../tests/services/integration/desktop_manager.rs"]
mod tests;
