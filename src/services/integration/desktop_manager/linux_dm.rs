use crate::{
    models::common::{DesktopEntry, enums::Filetype},
    services::artifact::AppImageExtractor,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::super::IconManager;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub(super) struct LinuxDesktopHandler {
    extractor: AppImageExtractor,
}

impl LinuxDesktopHandler {
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            extractor: AppImageExtractor::new()
                .context("Failed to initialize appimage extractor")?,
        })
    }

    pub(super) async fn add_icon<H>(
        &self,
        paths: &UpstreamPaths,
        name: &str,
        path: &Path,
        filetype: &Filetype,
        output_dir: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<Option<PathBuf>>
    where
        H: FnMut(&str),
    {
        IconManager::new(paths, &self.extractor)
            .add_icon_to(name, path, filetype, output_dir, message_callback)
            .await
    }

    pub(super) async fn create_entry<H>(
        &self,
        paths: &UpstreamPaths,
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
        self.write_entry(paths, &name, &entry)
    }

    pub(super) async fn create_staged_entry<H>(
        &self,
        _name: &str,
        staged_install_path: &Path,
        _staged_exec_path: Option<&Path>,
        _final_install_path: &Path,
        _final_exec_path: Option<&Path>,
        filetype: &Filetype,
        entry: DesktopEntry,
        entry_path: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let name = entry
            .name
            .as_deref()
            .ok_or_else(|| anyhow!("Desktop entry name is required"))?
            .to_string();
        let entry = if *filetype == Filetype::AppImage {
            let squashfs_root = self
                .extractor
                .extract(&name, staged_install_path, message_callback)
                .await?;
            let embedded = self
                .find_and_parse_desktop_file(&squashfs_root, &name, message_callback)
                .unwrap_or_default();
            Self::merge_embedded_entry(embedded, entry, &name)
        } else {
            entry.ensure_name(&name)
        };

        let mut entry = entry;
        entry.terminal = false;
        if let Some(parent) = entry_path.parent() {
            fs::create_dir_all(parent)?;
        }
        crate::utils::filesystem::atomic_ops::write_atomic(
            entry_path,
            entry.to_desktop_file().as_bytes(),
        )?;
        Ok(())
    }

    pub(super) fn remove_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        let path = Self::managed_entry_path(paths, name);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub(super) fn managed_entry_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        paths
            .integration
            .xdg_applications_dir
            .join(format!("{name}.desktop"))
    }

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

    fn write_entry(
        &self,
        paths: &UpstreamPaths,
        name: &str,
        entry: &DesktopEntry,
    ) -> Result<PathBuf> {
        let output_path = Self::managed_entry_path(paths, name);
        crate::utils::filesystem::atomic_ops::write_atomic(
            &output_path,
            entry.to_desktop_file().as_bytes(),
        )?;
        Ok(output_path)
    }

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
            squashfs_root.join(format!("{name}.desktop")),
            squashfs_root.join(format!("usr/share/applications/{name}.desktop")),
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
            found.sort_by_key(|path| {
                let stem = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
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
            entry.set_field(
                key.trim().trim_start_matches('\u{feff}'),
                value.trim().to_string(),
            );
        }
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::LinuxDesktopHandler;
    use crate::{
        models::{
            common::{
                DesktopEntry,
                enums::{Channel, Filetype, Provider},
            },
            upstream::Package,
        },
        services::integration::DesktopManager,
        utils::test_support,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("upstream-desktop-manager-test-{name}-{nanos}"))
    }

    fn package(name: &str, filetype: Filetype) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            filetype,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn parse_desktop_file_reads_desktop_entry_fields() {
        let root = temp_root("parse");
        fs::create_dir_all(&root).expect("create root");
        let path = root.join("tool.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nName=Tool\nExec=/usr/bin/tool\nIcon=tool\n\n[Other]\nName=Ignored\n",
        )
        .expect("write desktop file");

        let entry = LinuxDesktopHandler::parse_desktop_file(&path).expect("parse desktop file");
        assert_eq!(entry.name.as_deref(), Some("Tool"));
        assert_eq!(entry.exec.as_deref(), Some("/usr/bin/tool"));
        assert_eq!(entry.icon.as_deref(), Some("tool"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn embedded_primary_name_survives_generated_entry_merge() {
        let mut embedded = DesktopEntry::default();
        embedded.set_field("Name", "Embedded Tool".to_string());
        embedded.set_field("Comment", "Embedded comment".to_string());
        let mut generated = DesktopEntry::default();
        generated.set_field("Name", "Package Alias".to_string());
        generated.set_field("Exec", "/opt/tool".to_string());

        let merged = LinuxDesktopHandler::merge_embedded_entry(embedded, generated, "fallback");
        assert_eq!(merged.name.as_deref(), Some("Embedded Tool"));
        assert_eq!(merged.exec.as_deref(), Some("/opt/tool"));
        assert_eq!(merged.comment.as_deref(), Some("Embedded comment"));
    }

    #[tokio::test]
    async fn prepare_entry_uses_final_paths_and_keeps_live_paths_untouched() {
        let root = temp_root("staged-entry");
        let paths = test_support::upstream_paths(&root);
        let staged_root = root.join("candidate");
        fs::create_dir_all(&staged_root).expect("create candidate");
        let staged_exec = staged_root.join("tool");
        fs::write(&staged_exec, b"candidate binary").expect("write candidate");
        fs::write(staged_root.join("tool.svg"), b"candidate icon").expect("write icon");

        let final_root = paths.install.archives_dir.join("tool");
        let final_exec = final_root.join("tool");
        let mut staged = package("tool", Filetype::Archive);
        staged.install_path = Some(staged_root);
        staged.exec_path = Some(staged_exec);
        let mut final_package = staged.clone();
        final_package.install_path = Some(final_root);
        final_package.exec_path = Some(final_exec.clone());

        let staged_entry = root.join("workspace/desktop/tool.desktop");
        let staged_icons = root.join("workspace/icons");
        let manager = DesktopManager::new(&paths).expect("desktop manager");
        manager
            .prepare_package_entry(
                &staged,
                &mut final_package,
                &staged_entry,
                &staged_icons,
                &mut None::<fn(&str)>,
            )
            .await
            .expect("prepare staged entry");

        let entry = fs::read_to_string(&staged_entry).expect("read staged entry");
        assert!(entry.contains(&format!("Exec={}", final_exec.display())));
        assert!(staged_icons.join("tool.svg").exists());
        assert!(
            !DesktopManager::managed_entry_path(&paths, "tool")
                .expect("managed desktop path")
                .exists()
        );
        assert!(!paths.state.icons_dir.join("tool.svg").exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rename_refuses_to_overwrite_existing_entry() {
        let root = temp_root("rename");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.integration.xdg_applications_dir).expect("create applications");
        let old = paths.integration.xdg_applications_dir.join("old.desktop");
        let new = paths.integration.xdg_applications_dir.join("new.desktop");
        fs::write(&old, "old desktop\n").expect("write old");
        assert!(DesktopManager::rename_entry(&paths, "old", "new").expect("rename"));
        fs::write(&old, "another old\n").expect("write old again");
        let error = DesktopManager::rename_entry(&paths, "old", "new").expect_err("overwrite");
        assert!(error.to_string().contains("Refusing to overwrite"));
        assert_eq!(fs::read_to_string(&old).expect("read old"), "another old\n");
        assert_eq!(fs::read_to_string(&new).expect("read new"), "old desktop\n");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn snapshot_restores_entry_and_icon() {
        let root = temp_root("snapshot");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.integration.xdg_applications_dir).expect("create applications");
        fs::create_dir_all(&paths.state.icons_dir).expect("create icons");
        let entry_path = paths.integration.xdg_applications_dir.join("tool.desktop");
        let old_icon = paths.state.icons_dir.join("tool-old.png");
        let new_icon = paths.state.icons_dir.join("tool-new.png");
        fs::write(&entry_path, b"old desktop").expect("write entry");
        fs::write(&old_icon, b"old icon").expect("write icon");
        let mut previous = package("tool", Filetype::Binary);
        previous.icon_path = Some(old_icon.clone());
        let snapshot = DesktopManager::snapshot_for_package(&paths, &previous).expect("snapshot");
        fs::write(&entry_path, b"new desktop").expect("overwrite entry");
        fs::write(&old_icon, b"overwritten icon").expect("overwrite icon");
        fs::write(&new_icon, b"new icon").expect("write replacement icon");
        let mut replacement = previous;
        replacement.icon_path = Some(new_icon.clone());
        snapshot
            .restore(Some(&replacement))
            .expect("restore snapshot");
        assert_eq!(fs::read(&entry_path).expect("read entry"), b"old desktop");
        assert_eq!(fs::read(&old_icon).expect("read icon"), b"old icon");
        assert!(!new_icon.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
