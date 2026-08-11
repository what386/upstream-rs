use crate::{
    models::common::{DesktopEntry, enums::Filetype},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::super::IconManager;

pub(super) struct WindowsDesktopHandler;

impl WindowsDesktopHandler {
    pub(super) fn new() -> Self {
        Self
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
        IconManager::new(paths)
            .add_icon_to(name, path, filetype, output_dir, message_callback)
            .await
    }

    pub(super) async fn create_entry<H>(
        &self,
        paths: &UpstreamPaths,
        _install_path: &Path,
        _filetype: &Filetype,
        entry: DesktopEntry,
        _message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
        let name = entry
            .name
            .as_deref()
            .ok_or_else(|| anyhow!("Desktop entry name is required"))?;
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
        self.create_shortcut(paths, name, exec_path, icon_path)
    }

    pub(super) async fn create_staged_entry<H>(
        &self,
        name: &str,
        _staged_install_path: &Path,
        _staged_exec_path: Option<&Path>,
        _final_install_path: &Path,
        final_exec_path: Option<&Path>,
        _filetype: &Filetype,
        entry: DesktopEntry,
        entry_path: &Path,
        _message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let exec_path = final_exec_path
            .ok_or_else(|| anyhow!("Replacement package '{name}' has no executable path"))?;
        let icon_path = entry
            .icon
            .as_deref()
            .filter(|icon| !icon.is_empty())
            .map(Path::new);
        self.create_shortcut_at(entry_path, exec_path, icon_path)?;
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
        let shortcut_dir =
            dirs::desktop_dir().unwrap_or_else(|| paths.dirs.data_dir.join("shortcuts"));
        shortcut_dir.join(format!("{name}.lnk"))
    }

    fn create_shortcut(
        &self,
        paths: &UpstreamPaths,
        name: &str,
        exec_path: &Path,
        icon_path: Option<&Path>,
    ) -> Result<PathBuf> {
        self.create_shortcut_at(&Self::managed_entry_path(paths, name), exec_path, icon_path)
    }

    fn create_shortcut_at(
        &self,
        shortcut_path: &Path,
        exec_path: &Path,
        icon_path: Option<&Path>,
    ) -> Result<PathBuf> {
        if let Some(parent) = shortcut_path.parent() {
            fs::create_dir_all(parent).context("Failed to create shortcut directory")?;
        }
        let quote = |value: &str| value.replace('\'', "''");
        let target = quote(&exec_path.display().to_string());
        let shortcut = quote(&shortcut_path.display().to_string());
        let working_dir = exec_path
            .parent()
            .map(|path| quote(&path.display().to_string()))
            .unwrap_or_default();
        let mut script = vec![
            "$WshShell = New-Object -ComObject WScript.Shell".to_string(),
            format!("$Shortcut = $WshShell.CreateShortcut('{shortcut}')"),
            format!("$Shortcut.TargetPath = '{target}'"),
        ];
        if !working_dir.is_empty() {
            script.push(format!("$Shortcut.WorkingDirectory = '{working_dir}'"));
        }
        if let Some(icon) = icon_path {
            script.push(format!(
                "$Shortcut.IconLocation = '{},0'",
                quote(&icon.display().to_string())
            ));
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
        Ok(shortcut_path.to_path_buf())
    }
}
