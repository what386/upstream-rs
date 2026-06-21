#[cfg(unix)]
use crate::services::integration::{nushell_paths_file_contains_path, render_nushell_paths_file};
use crate::services::{
    integration::CompletionManager,
    storage::{
        config_storage::ConfigStorage, manifest_storage::ManifestStorage,
        trust_storage::TrustStorage,
    },
};
#[cfg(unix)]
use crate::utils::platform::shells::installed_shell_commands;
use crate::utils::static_paths::UpstreamPaths;
use crate::{output, output::Status};
#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
#[cfg(unix)]
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::path::Path;

// Unix shell source lines
#[cfg(unix)]
const SOURCE_LINE_BASH: &str =
    "[ -f $HOME/.upstream/metadata/paths.sh ] && source $HOME/.upstream/metadata/paths.sh";
#[cfg(unix)]
const SOURCE_LINE_FISH: &str =
    "test -f $HOME/.upstream/metadata/paths.sh; and source $HOME/.upstream/metadata/paths.sh";
#[cfg(unix)]
const SOURCE_LINE_NUSHELL: &str = r#"const upstream_paths_nu = if ("~/.upstream/metadata/paths.nu" | path expand | path exists) { ("~/.upstream/metadata/paths.nu" | path expand) } else { null }; source-env $upstream_paths_nu"#;

pub struct InitCheckReport {
    pub ok: bool,
    pub messages: Vec<String>,
}

fn check_ok(report: &mut InitCheckReport, detail: impl fmt::Display) {
    report
        .messages
        .push(format!("{} {}", output::status_cell(Status::Ok), detail));
}

fn check_fail(report: &mut InitCheckReport, detail: impl fmt::Display) {
    report.ok = false;
    report
        .messages
        .push(format!("{} {}", output::status_cell(Status::Fail), detail));
}

#[cfg(windows)]
fn normalize_windows_path(path: &str) -> String {
    let mut normalized = path.replace('/', "\\").trim().to_ascii_lowercase();
    while normalized.ends_with('\\') {
        normalized.pop();
    }
    normalized
}

pub fn initialize(paths: &UpstreamPaths) -> Result<()> {
    create_package_dirs(paths)?;
    create_manifest_file(paths)?;
    create_trust_file(paths)?;
    create_metadata_files(paths)?;
    create_default_config_file(paths)?;

    #[cfg(windows)]
    add_to_windows_path(paths)?;

    #[cfg(unix)]
    update_shell_profiles(paths)?;

    Ok(())
}

fn create_manifest_file(paths: &UpstreamPaths) -> Result<()> {
    ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))?.ensure_current()
}

fn create_trust_file(paths: &UpstreamPaths) -> Result<()> {
    TrustStorage::new(&paths.config.trust_file)?.ensure_exists()
}

pub fn purge_data(paths: &UpstreamPaths) -> Result<()> {
    if paths.dirs.data_dir.exists() {
        fs::remove_dir_all(&paths.dirs.data_dir)?;
    }
    Ok(())
}

pub fn check(paths: &UpstreamPaths) -> Result<InitCheckReport> {
    let mut report = InitCheckReport {
        ok: true,
        messages: Vec::new(),
    };

    for (label, path) in [
        ("config directory", &paths.dirs.config_dir),
        ("data directory", &paths.dirs.data_dir),
        ("metadata directory", &paths.dirs.metadata_dir),
        ("symlinks directory", &paths.integration.symlinks_dir),
        ("appimages directory", &paths.install.appimages_dir),
        ("binaries directory", &paths.install.binaries_dir),
        ("archives directory", &paths.install.archives_dir),
    ] {
        if path.exists() {
            check_ok(&mut report, format!("{} exists: {}", label, path.display()));
        } else {
            check_fail(
                &mut report,
                format!("{} missing: {}", label, path.display()),
            );
        }
    }

    let completion_manager = CompletionManager::new(paths);
    let completion_dirs = completion_manager.installed_shell_completion_dirs();
    if completion_dirs.is_empty() {
        check_ok(
            &mut report,
            "no supported shells detected for completion installation",
        );
    }
    for (shell, path) in completion_dirs {
        let label = format!("{shell} completions directory");
        if path.exists() {
            check_ok(&mut report, format!("{} exists: {}", label, path.display()));
        } else {
            check_fail(
                &mut report,
                format!("{} missing: {}", label, path.display()),
            );
        }
    }

    if paths.config.config_file.exists() {
        check_ok(
            &mut report,
            format!("config file exists: {}", paths.config.config_file.display()),
        );
    } else {
        check_fail(
            &mut report,
            format!(
                "config file missing: {}",
                paths.config.config_file.display()
            ),
        );
    }

    let manifest_file = ManifestStorage::path_for_root(&paths.dirs.data_dir);
    if manifest_file.exists() {
        check_ok(
            &mut report,
            format!("manifest file exists: {}", manifest_file.display()),
        );
    } else {
        check_fail(
            &mut report,
            format!("manifest file missing: {}", manifest_file.display()),
        );
    }

    if paths.config.trust_file.exists() {
        check_ok(
            &mut report,
            format!(
                "trust metadata file exists: {}",
                paths.config.trust_file.display()
            ),
        );
    } else {
        check_fail(
            &mut report,
            format!(
                "trust metadata file missing: {}",
                paths.config.trust_file.display()
            ),
        );
    }

    #[cfg(unix)]
    check_unix_integration(paths, &mut report)?;

    #[cfg(windows)]
    check_windows_integration(paths, &mut report)?;

    Ok(report)
}

#[cfg(windows)]
fn add_to_windows_path(paths: &UpstreamPaths) -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .context("Failed to open registry key")?;

    let symlinks_path = paths.integration.symlinks_dir.display().to_string();
    let symlinks_norm = normalize_windows_path(&symlinks_path);

    // Get current PATH
    let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

    // Check if our path is already in PATH
    let path_entries: Vec<&str> = current_path.split(';').collect();
    if path_entries
        .iter()
        .any(|&p| normalize_windows_path(p) == symlinks_norm)
    {
        return Ok(()); // Already in PATH
    }

    // Add our path to the beginning
    let new_path = if current_path.is_empty() {
        symlinks_path
    } else {
        format!("{};{}", symlinks_path, current_path)
    };

    env_key
        .set_value("Path", &new_path)
        .context("Failed to set PATH")?;

    // Broadcast WM_SETTINGCHANGE to notify other applications
    broadcast_environment_change();

    Ok(())
}

#[cfg(windows)]
fn broadcast_environment_change() {
    use std::ptr;
    use winapi::shared::minwindef::LPARAM;
    use winapi::um::winuser::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    unsafe {
        let env_string: Vec<u16> = "Environment\0".encode_utf16().collect();
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            env_string.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            5000,
            ptr::null_mut(),
        );
    }
}

fn create_package_dirs(paths: &UpstreamPaths) -> io::Result<()> {
    fs::create_dir_all(&paths.dirs.config_dir)?;
    fs::create_dir_all(&paths.dirs.data_dir)?;
    fs::create_dir_all(&paths.dirs.packages_dir)?;
    fs::create_dir_all(&paths.dirs.cache_dir)?;
    fs::create_dir_all(&paths.dirs.metadata_dir)?;
    fs::create_dir_all(&paths.install.appimages_dir)?;
    fs::create_dir_all(&paths.install.binaries_dir)?;
    fs::create_dir_all(&paths.install.archives_dir)?;
    fs::create_dir_all(&paths.install.tmp_dir)?;
    fs::create_dir_all(&paths.integration.icons_dir)?;
    fs::create_dir_all(&paths.integration.symlinks_dir)?;
    for (_shell, dir) in CompletionManager::new(paths).installed_shell_completion_dirs() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

fn create_default_config_file(paths: &UpstreamPaths) -> Result<()> {
    if paths.config.config_file.exists() {
        return Ok(());
    }

    let storage = ConfigStorage::new(&paths.config.config_file)?;
    storage.save_config()?;
    Ok(())
}

#[cfg(unix)]
fn create_metadata_files(paths: &UpstreamPaths) -> io::Result<()> {
    if !paths.config.paths_file.exists() {
        let export_line = format!(
            r#"export PATH="{}:$PATH""#,
            paths.integration.symlinks_dir.display()
        );
        fs::write(
            &paths.config.paths_file,
            format!(
                "#!/bin/bash\n# Upstream managed PATH additions\n{}\n",
                export_line
            ),
        )?;
    }
    if !paths.config.paths_nu_file.exists() {
        fs::write(
            &paths.config.paths_nu_file,
            render_nushell_paths_file(&[paths.integration.symlinks_dir.display().to_string()]),
        )?;
    }
    Ok(())
}

#[cfg(windows)]
fn create_metadata_files(_paths: &UpstreamPaths) -> io::Result<()> {
    // On Windows, we use registry-based PATH, so no metadata files needed
    Ok(())
}

#[cfg(unix)]
fn update_shell_profiles(paths: &UpstreamPaths) -> io::Result<()> {
    for shell in installed_shell_commands() {
        match shell.as_str() {
            "bash" | "sh" => {
                add_line_to_profile(paths, ".bashrc", SOURCE_LINE_BASH)?;
            }
            "zsh" => {
                add_line_to_profile(paths, ".zshrc", SOURCE_LINE_BASH)?;
            }
            "fish" => {
                let fish_config = Path::new(".config").join("fish").join("config.fish");
                add_line_to_profile(paths, &fish_config.to_string_lossy(), SOURCE_LINE_FISH)?;
            }
            "nu" => {
                let nushell_config = Path::new(".config").join("nushell").join("config.nu");
                add_line_to_profile(
                    paths,
                    &nushell_config.to_string_lossy(),
                    SOURCE_LINE_NUSHELL,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(unix)]
fn check_unix_integration(paths: &UpstreamPaths, report: &mut InitCheckReport) -> io::Result<()> {
    let expected_line = format!(
        r#"export PATH="{}:$PATH""#,
        paths.integration.symlinks_dir.display()
    );

    if !paths.config.paths_file.exists() {
        check_fail(
            report,
            format!(
                "PATH metadata file missing: {}",
                paths.config.paths_file.display()
            ),
        );
    } else {
        let content = fs::read_to_string(&paths.config.paths_file)?;
        if content.contains(&expected_line) {
            check_ok(
                report,
                format!(
                    "PATH metadata file contains symlink export: {}",
                    paths.config.paths_file.display()
                ),
            );
        } else {
            check_fail(
                report,
                format!(
                    "PATH metadata file missing expected export line: {}",
                    paths.config.paths_file.display()
                ),
            );
        }
    }

    let expected_nushell_path = paths.integration.symlinks_dir.display().to_string();

    if !paths.config.paths_nu_file.exists() {
        check_fail(
            report,
            format!(
                "Nushell PATH metadata file missing: {}",
                paths.config.paths_nu_file.display()
            ),
        );
    } else {
        let content = fs::read_to_string(&paths.config.paths_nu_file)?;
        if nushell_paths_file_contains_path(&content, &expected_nushell_path) {
            check_ok(
                report,
                format!(
                    "Nushell PATH metadata file contains symlink path: {}",
                    paths.config.paths_nu_file.display()
                ),
            );
        } else {
            check_fail(
                report,
                format!(
                    "Nushell PATH metadata file missing expected symlink path: {}",
                    paths.config.paths_nu_file.display()
                ),
            );
        }
    }

    let mut profiles_to_check: BTreeSet<(String, String)> = BTreeSet::new();
    for shell in installed_shell_commands() {
        match shell.as_str() {
            "bash" | "sh" => {
                profiles_to_check.insert((".bashrc".to_string(), SOURCE_LINE_BASH.to_string()));
            }
            "zsh" => {
                profiles_to_check.insert((".zshrc".to_string(), SOURCE_LINE_BASH.to_string()));
            }
            "fish" => {
                profiles_to_check.insert((
                    ".config/fish/config.fish".to_string(),
                    SOURCE_LINE_FISH.to_string(),
                ));
            }
            "nu" => {
                profiles_to_check.insert((
                    ".config/nushell/config.nu".to_string(),
                    SOURCE_LINE_NUSHELL.to_string(),
                ));
            }
            _ => {}
        }
    }

    for (profile_rel, expected_line) in profiles_to_check {
        let profile_path = paths.dirs.user_dir.join(&profile_rel);
        if !profile_path.exists() {
            check_fail(
                report,
                format!("Shell profile missing: {}", profile_path.display()),
            );
            continue;
        }

        let content = fs::read_to_string(&profile_path)?;
        if content.contains(&expected_line) {
            check_ok(
                report,
                format!(
                    "Shell profile contains upstream hook: {}",
                    profile_path.display()
                ),
            );
        } else {
            check_fail(
                report,
                format!(
                    "Shell profile missing upstream hook: {}",
                    profile_path.display()
                ),
            );
        }
    }

    Ok(())
}

#[cfg(unix)]
fn add_line_to_profile(paths: &UpstreamPaths, relative_path: &str, line: &str) -> io::Result<()> {
    let profile_path = paths.dirs.user_dir.join(relative_path);

    // Ensure parent directory exists
    if let Some(parent) = profile_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Backup original file
    if profile_path.exists() {
        let backup_path = profile_path.with_extension("bak");
        if !backup_path.exists() {
            fs::copy(&profile_path, &backup_path)?;
        }
    }

    if !profile_path.exists() {
        fs::write(&profile_path, format!("{}\n", line))?;
        return Ok(());
    }

    let content = fs::read_to_string(&profile_path)?;
    if !content.contains(line) {
        let mut file = fs::OpenOptions::new().append(true).open(&profile_path)?;
        writeln!(file, "\n{}", line)?;
    }

    Ok(())
}

#[cfg(unix)]
pub fn cleanup(paths: &UpstreamPaths) -> Result<()> {
    for shell in installed_shell_commands() {
        let profile = match shell.as_str() {
            "bash" | "sh" => Some(".bashrc"),
            "zsh" => Some(".zshrc"),
            "fish" => Some(".config/fish/config.fish"),
            "nu" => Some(".config/nushell/config.nu"),
            _ => None,
        };
        if let Some(profile_rel) = profile {
            let profile_path = paths.dirs.user_dir.join(profile_rel);
            if !profile_path.exists() {
                continue;
            }
            let mut content = fs::read_to_string(&profile_path)?;
            content = content
                .replace(&format!("{}\n", SOURCE_LINE_BASH), "")
                .replace(SOURCE_LINE_BASH, "")
                .replace(&format!("{}\n", SOURCE_LINE_FISH), "")
                .replace(SOURCE_LINE_FISH, "")
                .replace(&format!("{}\n", SOURCE_LINE_NUSHELL), "")
                .replace(SOURCE_LINE_NUSHELL, "");
            fs::write(&profile_path, content)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn cleanup(paths: &UpstreamPaths) -> Result<()> {
    remove_from_windows_path(paths)
}

#[cfg(windows)]
fn remove_from_windows_path(paths: &UpstreamPaths) -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .context("Failed to open registry key")?;

    let symlinks_path = paths.integration.symlinks_dir.display().to_string();
    let symlinks_norm = normalize_windows_path(&symlinks_path);

    // Get current PATH
    let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

    // Remove our path from PATH
    let path_entries: Vec<&str> = current_path
        .split(';')
        .filter(|&p| normalize_windows_path(p) != symlinks_norm)
        .collect();

    let new_path = path_entries.join(";");

    env_key
        .set_value("Path", &new_path)
        .context("Failed to set PATH")?;

    // Broadcast WM_SETTINGCHANGE to notify other applications
    broadcast_environment_change();

    Ok(())
}

#[cfg(windows)]
fn check_windows_integration(paths: &UpstreamPaths, report: &mut InitCheckReport) -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ)
        .context("Failed to open PATH")?;

    let symlinks_path = paths.integration.symlinks_dir.display().to_string();
    let symlinks_norm = normalize_windows_path(&symlinks_path);
    let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

    let in_path = current_path
        .split(';')
        .any(|p| normalize_windows_path(p) == symlinks_norm);

    if in_path {
        check_ok(report, "Windows PATH contains upstream symlinks directory");
    } else {
        check_fail(report, "Windows PATH missing upstream symlinks directory");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::purge_data;
    use crate::services::storage::manifest_storage::{CURRENT_LAYOUT_VERSION, MANIFEST_FILE_NAME};
    use crate::utils::static_paths::{
        AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths,
    };
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-init-test-{name}-{nanos}"))
    }

    fn test_paths(root: &Path) -> UpstreamPaths {
        let dirs = AppDirs {
            user_dir: root.to_path_buf(),
            config_dir: root.join("config"),
            data_dir: root.join(".upstream"),
            packages_dir: root.join(".upstream/packages"),
            cache_dir: root.join(".upstream/cache"),
            metadata_dir: root.join(".upstream/metadata"),
        };

        UpstreamPaths {
            config: ConfigPaths {
                config_file: dirs.config_dir.join("config.toml"),
                packages_file: dirs.metadata_dir.join("packages.json"),
                trust_file: dirs.metadata_dir.join("trust.json"),
                paths_file: dirs.metadata_dir.join("paths.sh"),
                paths_nu_file: dirs.metadata_dir.join("paths.nu"),
            },
            install: InstallPaths {
                appimages_dir: dirs.packages_dir.join("appimages"),
                binaries_dir: dirs.packages_dir.join("binaries"),
                archives_dir: dirs.packages_dir.join("archives"),
                rollback_dir: dirs.data_dir.join("rollback"),
                tmp_dir: dirs.data_dir.join("tmp"),
            },
            integration: IntegrationPaths {
                symlinks_dir: dirs.data_dir.join("symlinks"),
                xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
                icons_dir: dirs.data_dir.join("icons"),
                bash_completions_dir: dirs
                    .user_dir
                    .join(".local/share/bash-completion/completions"),
                fish_completions_dir: dirs.user_dir.join(".config/fish/completions"),
                zsh_completions_dir: dirs.user_dir.join(".local/share/zsh/site-functions"),
            },
            dirs,
        }
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }

    #[test]
    fn create_manifest_file_writes_current_layout_manifest() {
        let root = temp_root("manifest-file");
        let paths = test_paths(&root);

        super::create_manifest_file(&paths).expect("create manifest file");

        let manifest_path = paths.dirs.data_dir.join(MANIFEST_FILE_NAME);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
                .expect("parse manifest");
        assert_eq!(
            manifest["layout_version"].as_u64(),
            Some(CURRENT_LAYOUT_VERSION as u64)
        );
        assert_eq!(
            manifest["platform"]["os"].as_str(),
            Some(std::env::consts::OS)
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn create_trust_file_writes_empty_trust_storage() {
        let root = temp_root("trust-file");
        let paths = test_paths(&root);

        super::create_trust_file(&paths).expect("create trust file");

        let trust: serde_json::Value =
            serde_json::from_slice(&fs::read(&paths.config.trust_file).expect("read trust file"))
                .expect("parse trust file");
        assert_eq!(trust["version"].as_u64(), Some(1));
        assert_eq!(
            trust["minisign_public_keys"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(
            trust["cosign_public_keys"].as_array().map(Vec::len),
            Some(0)
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn check_reports_manifest_file_status() {
        let root = temp_root("manifest-check");
        let paths = test_paths(&root);

        let missing_report = super::check(&paths).expect("check missing manifest");
        assert!(
            missing_report
                .messages
                .iter()
                .map(|message| console::strip_ansi_codes(message).to_string())
                .any(|message| message.contains("[fail]")
                    && message.contains("manifest file missing"))
        );

        fs::create_dir_all(&paths.dirs.data_dir).expect("create data dir");
        super::create_manifest_file(&paths).expect("create manifest");
        let present_report = super::check(&paths).expect("check present manifest");
        assert!(
            present_report
                .messages
                .iter()
                .map(|message| console::strip_ansi_codes(message).to_string())
                .any(|message| message.contains("[ok]") && message.contains("manifest file exists"))
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn check_reports_trust_file_status() {
        let root = temp_root("trust-check");
        let paths = test_paths(&root);

        let missing_report = super::check(&paths).expect("check missing trust");
        assert!(
            missing_report
                .messages
                .iter()
                .map(|message| console::strip_ansi_codes(message).to_string())
                .any(|message| message.contains("[fail]")
                    && message.contains("trust metadata file missing"))
        );

        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata dir");
        super::create_trust_file(&paths).expect("create trust");
        let present_report = super::check(&paths).expect("check present trust");
        assert!(
            present_report
                .messages
                .iter()
                .map(|message| console::strip_ansi_codes(message).to_string())
                .any(|message| message.contains("[ok]")
                    && message.contains("trust metadata file exists"))
        );

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn create_metadata_files_creates_posix_and_nushell_path_files() {
        let root = temp_root("metadata-files");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata dir");

        super::create_metadata_files(&paths).expect("create metadata files");

        let posix_content = fs::read_to_string(&paths.config.paths_file).expect("read paths.sh");
        assert!(posix_content.contains("export PATH="));
        assert!(posix_content.contains(&paths.integration.symlinks_dir.display().to_string()));

        let nushell_content =
            fs::read_to_string(&paths.config.paths_nu_file).expect("read paths.nu");
        assert!(nushell_content.contains("let upstream_paths = ["));
        assert!(nushell_content.contains("$env.PATH = ($upstream_paths ++ $env.PATH)"));
        assert!(nushell_content.contains(&paths.integration.symlinks_dir.display().to_string()));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn purge_data_removes_data_dir_but_keeps_config_dir() {
        let root = temp_root("purge");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.dirs.data_dir).expect("create data dir");
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config dir");
        fs::write(paths.dirs.data_dir.join("data"), b"data").expect("write data");
        fs::write(paths.dirs.config_dir.join("config.toml"), b"").expect("write config");

        purge_data(&paths).expect("purge data");

        assert!(!paths.dirs.data_dir.exists());
        assert!(paths.dirs.config_dir.exists());

        cleanup(&root).expect("cleanup");
    }
}
