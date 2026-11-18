use super::super::StagedDesktopEntry;
use super::WindowsDesktopHandler;
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

impl WindowsDesktopHandler {
    pub(in super::super) async fn create_entry<H>(
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

    pub(in super::super) async fn create_staged_entry<H>(
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

    /// Update existing shortcuts without changing their names, icons or working directories.
    pub(in super::super) fn update_launch_commands(
        directory: &Path,
        package: &crate::models::upstream::Package,
        template: Option<&str>,
    ) -> Result<()> {
        use crate::services::integration::LauncherManager;
        if !directory.is_dir() {
            return Ok(());
        }
        let command = template
            .map(|template| {
                let executable = package.primary_executable().ok_or_else(|| {
                    anyhow!(
                        "Package '{}' has no executable for its shortcut",
                        package.id
                    )
                })?;
                LauncherManager::format_windows_command(
                    &LauncherManager::validate(template)?,
                    &executable.path,
                )
            })
            .transpose()?;
        let quote = |value: &str| value.replace('\'', "''");
        let mut script = vec![
            "$ErrorActionPreference = 'Stop'".to_string(),
            "$WshShell = New-Object -ComObject WScript.Shell".to_string(),
            format!(
                "$Links = @(Get-ChildItem -LiteralPath '{}' -Filter '*.lnk' -File)",
                quote(&directory.to_string_lossy())
            ),
        ];
        if let Some((target, arguments)) = command {
            script.push(format!("$Target = '{}'", quote(&target)));
            // Shortcut targets must be full paths; resolve an explicitly supplied prefix on PATH.
            script.push("if (-not [System.IO.Path]::IsPathRooted($Target)) { $Target = (Get-Command -Name $Target -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source }".to_string());
            script.push(format!("$Arguments = '{}'", quote(&arguments)));
            script.push("foreach ($Link in $Links) { $Shortcut = $WshShell.CreateShortcut($Link.FullName); $OriginalPath = $Link.FullName + '.launch.json'; if (Test-Path -LiteralPath $OriginalPath) { $Original = Get-Content -LiteralPath $OriginalPath -Raw | ConvertFrom-Json } else { $Original = [PSCustomObject]@{ TargetPath = $Shortcut.TargetPath; Arguments = $Shortcut.Arguments }; $Original | ConvertTo-Json -Compress | Set-Content -LiteralPath $OriginalPath -Encoding UTF8 }; $Shortcut.TargetPath = $Target; $Shortcut.Arguments = $Arguments; if (-not [string]::IsNullOrEmpty($Original.Arguments)) { $Shortcut.Arguments += ' ' + $Original.Arguments }; $Shortcut.Save() }".to_string());
        } else {
            script.push("foreach ($Link in $Links) { $OriginalPath = $Link.FullName + '.launch.json'; if (Test-Path -LiteralPath $OriginalPath) { $Original = Get-Content -LiteralPath $OriginalPath -Raw | ConvertFrom-Json; $Shortcut = $WshShell.CreateShortcut($Link.FullName); $Shortcut.TargetPath = $Original.TargetPath; $Shortcut.Arguments = $Original.Arguments; $Shortcut.Save(); Remove-Item -LiteralPath $OriginalPath } }".to_string());
        }
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
            .context("Failed to execute PowerShell to update shortcut launch options")?;
        anyhow::ensure!(
            output.status.success(),
            "Failed to update Windows shortcut launch options: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }

    pub(in super::super) fn remove_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
        let path = Self::managed_entry_path(paths, name);
        if path.exists() {
            fs::remove_dir_all(path)?;
        }

        Self::remove_legacy_entry(paths, name)
    }

    // Treat the package directory as one integration artifact for activation,
    // rollback and removal. Only the .lnk inside it has a user-facing name.
    pub(in super::super) fn managed_entry_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        paths
            .integration
            .windows_programs_dir
            .join("Upstream")
            .join(filesystem_name(name))
    }

    pub(in super::super) fn legacy_entry_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        dirs::desktop_dir()
            .unwrap_or_else(|| paths.dirs.data_dir.join("shortcuts"))
            .join(format!("{}.lnk", filesystem_name(name)))
    }

    pub(in super::super) fn remove_legacy_entry(paths: &UpstreamPaths, name: &str) -> Result<()> {
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
            // CreateShortcut opens an existing link too. Clear its previous launch
            // options before recording the fresh command as the new baseline.
            "$Shortcut.Arguments = ''".to_string(),
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
        script.push(format!("Get-ChildItem -LiteralPath '{directory_arg}' -Filter '*.lnk.launch.json' -File | Remove-Item -Force"));
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

    fn inspect_shortcut(path: &Path, arguments: Option<&str>) -> serde_json::Value {
        let quote = |value: &str| value.replace('\'', "''");
        let update = arguments
            .map(|arguments| format!("$s.Arguments = '{}'; $s.Save();", quote(arguments)))
            .unwrap_or_default();
        let script = format!(
            "$ErrorActionPreference = 'Stop'; $s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); {update} $s | Select-Object TargetPath, Arguments, Description, WorkingDirectory, IconLocation | ConvertTo-Json -Compress",
            quote(&path.to_string_lossy())
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    #[tokio::test]
    async fn windows_launch_options_update_restore_and_apply_to_prepared_shortcuts() {
        use crate::{
            application::{cli::arguments::PackageSettingKey, operations::package_op},
            models::{
                common::enums::{Channel, Provider},
                upstream::{Package, PackageExecutable},
            },
            storage::database::PackageDatabase,
        };
        let root = temp_root("upstream-shortcuts", "O'Brien launch options");
        let mut paths = upstream_paths(&root);
        paths.integration.windows_programs_dir = root.join("Programs");
        fs::create_dir_all(&root).unwrap();
        let binary = root.join("app.exe");
        fs::write(
            &binary,
            include_bytes!(
                "../../../../../tests/fixtures/artifacts/windows/windows-version-info.bin"
            ),
        )
        .unwrap();
        let mut package = Package::with_defaults(
            "tool".into(),
            "owner/tool".into(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(binary.clone());
        package.executables.push(PackageExecutable {
            name: "tool".into(),
            path: binary.clone(),
        });
        let mut database = PackageDatabase::open(&paths.metadata.packages_database_file).unwrap();
        database.upsert_package(&package).unwrap();
        let directory = WindowsDesktopHandler::managed_entry_path(&paths, &package.id);
        let apply = |template: &str| {
            package_op::set(
                "tool",
                vec![format!("desktop_launch_options={template}")],
                &paths,
            )
        };
        apply(r#"{} --profile "Work Profile""#).unwrap();
        assert!(!directory.exists());
        package_op::add_entry("tool", &paths).await.unwrap();
        let shortcut = directory.join("Example Launcher.lnk");
        let first = inspect_shortcut(&shortcut, None);
        assert_eq!(first["TargetPath"], binary.to_str().unwrap());
        assert_eq!(first["Arguments"], r#"--profile "Work Profile""#);
        // Recreating a configured shortcut must not accumulate flags.
        package_op::add_entry("tool", &paths).await.unwrap();
        assert_eq!(inspect_shortcut(&shortcut, None), first);
        let prefix = std::env::current_exe().unwrap();
        apply(&format!("\"{}\" {{}} --flag", prefix.display())).unwrap();
        let prefixed = inspect_shortcut(&shortcut, None);
        assert_eq!(prefixed["TargetPath"], prefix.to_str().unwrap());
        assert!(
            prefixed["Arguments"]
                .as_str()
                .unwrap()
                .contains(binary.to_str().unwrap())
        );
        for field in ["Description", "WorkingDirectory", "IconLocation"] {
            assert_eq!(prefixed[field], first[field]);
        }
        let before = fs::read(&shortcut).unwrap();
        assert!(apply("upstream-nonexistent-launch-prefix-94783.exe {} --bad").is_err());
        assert_eq!(fs::read(&shortcut).unwrap(), before);
        assert!(
            !package_op::get("tool", &paths)
                .unwrap()
                .1
                .desktop_launch_options
                .unwrap()
                .contains("--bad")
        );
        package_op::unset(
            "tool",
            vec![PackageSettingKey::DesktopLaunchOptions],
            &paths,
        )
        .unwrap();
        let original = inspect_shortcut(&shortcut, Some(r#"--original "Original Profile""#));
        apply("{} --one").unwrap();
        apply("{} --two").unwrap();
        assert_eq!(
            inspect_shortcut(&shortcut, None)["Arguments"],
            r#"--two --original "Original Profile""#
        );
        package_op::unset(
            "tool",
            vec![PackageSettingKey::DesktopLaunchOptions],
            &paths,
        )
        .unwrap();
        assert_eq!(inspect_shortcut(&shortcut, None), original);
        assert!(
            !shortcut
                .with_file_name("Example Launcher.lnk.launch.json")
                .exists()
        );

        // Upgrade preparation reads version info from staging but formats the final path.
        apply("{} --flag").unwrap();
        let mut final_package = package.clone();
        let final_binary = root.join("future/app.exe");
        final_package.install_path = Some(final_binary.clone());
        final_package.executables[0].path = final_binary.clone();
        let prepared = root.join("prepared");
        DesktopManager::new(&paths)
            .unwrap()
            .prepare_package_entry(
                &package,
                &mut final_package,
                &prepared,
                &root.join("icons"),
                &mut None::<fn(&str)>,
            )
            .await
            .unwrap();
        let staged = inspect_shortcut(&prepared.join("Example Launcher.lnk"), None);
        assert_eq!(staged["TargetPath"], final_binary.to_str().unwrap());
        assert_eq!(staged["Arguments"], "--flag");
        assert!(!final_binary.exists());
        fs::remove_dir_all(root).unwrap();
    }

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
        let snapshot = super::super::super::FileSnapshot::capture(directory.clone()).unwrap();
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
        let fixture = include_bytes!(
            "../../../../../tests/fixtures/artifacts/windows/windows-version-info.bin"
        );

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
