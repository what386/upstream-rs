use crate::{
    models::common::{DesktopEntry, enums::Filetype},
    providers::assets::filenames::filesystem_name,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::super::IconManager;
use super::StagedDesktopEntry;

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
        package_id: &str,
        _install_path: &Path,
        _filetype: &Filetype,
        entry: DesktopEntry,
        _message_callback: &mut Option<H>,
    ) -> Result<PathBuf>
    where
        H: FnMut(&str),
    {
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

        let directory = Self::managed_entry_path(paths, package_id);
        self.create_shortcut_at(
            &directory,
            exec_path,
            exec_path,
            icon_path,
            entry.name.as_deref().unwrap_or("Application"),
        )?;
        Self::remove_legacy_entry(paths, package_id)?;
        Ok(directory)
    }

    pub(super) async fn create_staged_entry<H>(
        &self,
        request: StagedDesktopEntry<'_>,
        _message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let exec_path = request
            .entry
            .exec
            .as_deref()
            .map(Path::new)
            .ok_or_else(|| anyhow!("Desktop entry exec path is required"))?;

        let icon_path = request
            .entry
            .icon
            .as_deref()
            .filter(|icon| !icon.is_empty())
            .map(Path::new);

        self.create_shortcut_at(
            request.path,
            exec_path,
            request.staged_executable.unwrap_or(exec_path),
            icon_path,
            request.entry.name.as_deref().unwrap_or("Application"),
        )?;
        Ok(())
    }

    pub(super) fn remove_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        let path = Self::managed_entry_path(paths, name);
        if path.exists() {
            fs::remove_dir_all(path)?;
        }

        Self::remove_legacy_entry(paths, name)
    }

    // Treat the package directory as one integration artifact for activation,
    // rollback and removal. Only the .lnk inside it has a user-facing name.
    pub(super) fn managed_entry_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        paths
            .integration
            .windows_programs_dir
            .join("Upstream")
            .join(filesystem_name(name))
    }

    pub(super) fn legacy_entry_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        dirs::desktop_dir()
            .unwrap_or_else(|| paths.dirs.data_dir.join("shortcuts"))
            .join(format!("{}.lnk", filesystem_name(name)))
    }

    pub(super) fn remove_legacy_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        let path = Self::legacy_entry_path(paths, name);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "Failed to remove legacy Windows shortcut '{}'",
                    path.display()
                )
            }),
        }
    }

    fn create_shortcut_at(
        &self,
        directory: &Path,
        exec_path: &Path,
        metadata_path: &Path,
        icon_path: Option<&Path>,
        fallback_name: &str,
    ) -> Result<()> {
        fs::create_dir_all(directory).context("Failed to create shortcut directory")?;

        let quote = |value: &str| value.replace('\'', "''");
        let target = quote(&exec_path.display().to_string());
        let directory_arg = quote(&directory.display().to_string());
        let metadata = quote(&metadata_path.display().to_string());
        let fallback = quote(fallback_name);
        let working_dir = exec_path
            .parent()
            .map(|path| quote(&path.display().to_string()))
            .unwrap_or_default();

        let mut script = vec![
            "$ErrorActionPreference = 'Stop'".to_string(),
            // Read the staged executable during preparation; TargetPath still
            // points at the final install location, which may not exist yet.
            format!("$Version = [System.Diagnostics.FileVersionInfo]::GetVersionInfo('{metadata}')"),
            format!("$Name = @($Version.ProductName, $Version.FileDescription, '{fallback}') | Where-Object {{ -not [string]::IsNullOrWhiteSpace($_) }} | Select-Object -First 1"),
            "$Name = $Name.Trim() -replace '[<>:\"/\\\\|?*\\x00-\\x1f]', '_'".to_string(),
            "$Name = $Name.TrimEnd([char[]]' .'); if ([string]::IsNullOrWhiteSpace($Name)) { $Name = 'Application' }".to_string(),
            // Prefix reserved DOS device names even when they have an extension.
            "if ($Name -match '^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])($|\\.)') { $Name = '_' + $Name }".to_string(),
            format!("$ShortcutPath = Join-Path '{directory_arg}' ($Name + '.lnk')"),
            "$WshShell = New-Object -ComObject WScript.Shell".to_string(),
            "$Shortcut = $WshShell.CreateShortcut($ShortcutPath)".to_string(),
            format!("$Shortcut.TargetPath = '{target}'"),
            "$Shortcut.Description = $Name".to_string(),
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
        script.push("if (-not (Test-Path -LiteralPath $ShortcutPath -PathType Leaf)) { throw 'Shortcut was not created' }".to_string());
        // A changed ProductName must not leave duplicate launchers behind.
        script.push(format!("Get-ChildItem -LiteralPath '{directory_arg}' -Filter '*.lnk' | Where-Object {{ $_.FullName -ne $ShortcutPath }} | Remove-Item -Force"));

        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script.join("; "),
            ])
            .output()
            .context("Failed to execute PowerShell for shortcut creation")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create Windows shortcut in '{}' (PowerShell exit status: {}): {}",
                directory.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        anyhow::ensure!(
            fs::read_dir(directory)?.any(|entry| entry.is_ok_and(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
                    && entry.path().is_file()
            })),
            "PowerShell reported success but no Windows shortcut exists in '{}'",
            directory.display()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::integration::DesktopManager;
    use crate::utils::test_support::{temp_root, upstream_paths};

    #[test]
    fn start_menu_package_directory_preserves_friendly_names_on_rename_and_removal() {
        let root = temp_root("upstream-shortcuts", "lifecycle");
        let mut paths = upstream_paths(&root);
        paths.integration.windows_programs_dir = root.join("redirected/Programs");
        let old_id = "github:example/launcher";
        let new_id = "github:example/renamed-launcher";
        let directory = WindowsDesktopHandler::managed_entry_path(&paths, old_id);
        assert_eq!(
            directory,
            root.join("redirected/Programs/Upstream/github_example_launcher")
        );
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("Example Launcher.lnk"), b"shortcut").unwrap();
        assert!(DesktopManager::entry_exists(&paths, old_id).unwrap());
        let snapshot = super::super::FileSnapshot::capture(directory.clone()).unwrap();
        fs::remove_dir_all(&directory).unwrap();
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("Replacement.lnk"), b"replacement").unwrap();
        snapshot.restore().unwrap();
        assert_eq!(
            fs::read(directory.join("Example Launcher.lnk")).unwrap(),
            b"shortcut"
        );
        assert!(!directory.join("Replacement.lnk").exists());
        assert!(DesktopManager::rename_entry(&paths, old_id, new_id).unwrap());
        let renamed = WindowsDesktopHandler::managed_entry_path(&paths, new_id);
        assert!(renamed.join("Example Launcher.lnk").is_file());
        assert!(!directory.exists());
        WindowsDesktopHandler::remove_entry(&paths, new_id).unwrap();
        assert!(!renamed.exists());
        assert!(!DesktopManager::entry_exists(&paths, new_id).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires native Windows PowerShell and WScript.Shell COM; run in Windows CI"]
    async fn windows_shortcut_uses_staged_metadata_and_final_target() {
        let root = temp_root("upstream-shortcuts", "O'Brien");
        fs::create_dir_all(&root).unwrap();
        let metadata_path = root.join("staged.exe");
        let final_target = root.join("not-installed-yet/launcher.exe");
        let entry_path = root.join("prepared");
        let fixture =
            include_bytes!("../../../../tests/fixtures/artifacts/windows/windows-version-info.bin");
        fs::write(&metadata_path, fixture).unwrap();
        let handler = WindowsDesktopHandler::new();
        let mut entry = DesktopEntry::new("craft-launcher");
        entry.exec = Some(final_target.display().to_string());
        handler
            .create_staged_entry(
                StagedDesktopEntry {
                    staged_install_path: &root,
                    staged_executable: Some(&metadata_path),
                    filetype: &Filetype::Archive,
                    entry,
                    path: &entry_path,
                },
                &mut None::<fn(&str)>,
            )
            .await
            .unwrap();

        let shortcut = entry_path.join("Example Launcher.lnk");
        assert!(shortcut.is_file());
        assert!(!final_target.exists());
        // Inspect the actual COM shortcut, including apostrophes in its path.
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &format!(
                "$ErrorActionPreference = 'Stop'; $s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath",
                shortcut.display().to_string().replace('\'', "''")
            )]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            final_target.to_str().unwrap()
        );

        // Blank ProductName should use FileDescription and remove the old name.
        let mut description_fixture = fixture.to_vec();
        let product: Vec<u8> = "Example Launcher"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let offset = description_fixture
            .windows(product.len())
            .position(|bytes| bytes == product)
            .unwrap();
        for pair in description_fixture[offset..offset + product.len()].chunks_exact_mut(2) {
            pair.copy_from_slice(&[b' ', 0]);
        }
        fs::write(&metadata_path, description_fixture).unwrap();
        handler
            .create_shortcut_at(
                &entry_path,
                &final_target,
                &metadata_path,
                None,
                "craft-launcher",
            )
            .unwrap();
        assert!(entry_path.join("Example Description.lnk").is_file());
        assert!(!shortcut.exists());

        // No version resources should use the clean repository name.
        fs::copy(std::env::current_exe().unwrap(), &metadata_path).unwrap();
        handler
            .create_shortcut_at(
                &entry_path,
                &final_target,
                &metadata_path,
                None,
                "craft-launcher",
            )
            .unwrap();
        assert!(entry_path.join("craft-launcher.lnk").is_file());
        assert_eq!(fs::read_dir(&entry_path).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
