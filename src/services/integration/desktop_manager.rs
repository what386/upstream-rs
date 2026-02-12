use crate::{
    models::common::{DesktopEntry, enums::Filetype},
    services::integration::appimage_extractor::AppImageExtractor,
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
#[cfg(windows)]
use anyhow::Context;
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
    #[cfg(unix)]
    extractor: &'a AppImageExtractor,
}

impl<'a> DesktopManager<'a> {
    pub fn new(paths: &'a UpstreamPaths, extractor: &'a AppImageExtractor) -> Self {
        Self {
            paths,
            #[cfg(unix)]
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
        #[cfg(unix)]
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
        #[cfg(unix)]
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

        #[cfg(windows)]
        {
            let path = Self::windows_shortcut_path(paths, name);
            if path.exists() {
                fs::remove_file(&path)?;
            }
            return Ok(());
        }
    }

    #[cfg(unix)]
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
        let entry = if *filetype == Filetype::AppImage {
            let squashfs_root = self
                .extractor
                .extract(name, install_path, message_callback)
                .await?;
            self.find_and_parse_desktop_file(&squashfs_root, name, message_callback)
                .unwrap_or_default()
        } else {
            DesktopEntry {
                name: Some(name.to_string()),
                comment: comment.map(String::from),
                categories: categories.map(String::from),
                ..DesktopEntry::default()
            }
        };

        let entry = entry.sanitize(exec_path, icon_path);
        self.write_unix_entry(name, &entry)
    }

    #[cfg(unix)]
    fn write_unix_entry(&self, name: &str, entry: &DesktopEntry) -> Result<PathBuf> {
        let out_path = self
            .paths
            .integration
            .xdg_applications_dir
            .join(format!("{}.desktop", name));
        fs::write(&out_path, entry.to_desktop_file())?;
        Ok(out_path)
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
    fn parse_desktop_file(path: &Path) -> Option<DesktopEntry> {
        let content = fs::read_to_string(path).ok()?;
        let mut entry = DesktopEntry::default();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.starts_with('[') || !trimmed.contains('=') {
                continue;
            }
            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };
            let value = value.trim().to_string();
            match key.trim() {
                "Name" => entry.name = Some(value),
                "Comment" => entry.comment = Some(value),
                "Exec" => entry.exec = Some(value),
                "Icon" => entry.icon = Some(value),
                "Categories" => entry.categories = Some(value),
                "Terminal" => entry.terminal = value.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }

        Some(entry)
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
