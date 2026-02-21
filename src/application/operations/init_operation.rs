use crate::utils::static_paths::UpstreamPaths;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

// Unix shell source lines
const SOURCE_LINE_BASH: &str =
    "[ -f $HOME/.upstream/metadata/paths.sh ] && source $HOME/.upstream/metadata/paths.sh";
const SOURCE_LINE_FISH: &str = "source $HOME/.upstream/metadata/paths.sh";

pub struct InitCheckReport {
    pub ok: bool,
    pub messages: Vec<String>,
}

#[cfg(windows)]
fn normalize_windows_path(path: &str) -> String {
    let mut normalized = path.replace('/', "\\").trim().to_ascii_lowercase();
    while normalized.ends_with('\\') {
        normalized.pop();
    }
    normalized
}

pub fn initialize(paths: &UpstreamPaths) -> io::Result<()> {
    create_package_dirs(paths)?;
    create_metadata_files(paths)?;

    #[cfg(windows)]
    add_to_windows_path(paths)?;

    #[cfg(unix)]
    update_shell_profiles(paths)?;

    Ok(())
}

pub fn check(paths: &UpstreamPaths) -> io::Result<InitCheckReport> {
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
            report
                .messages
                .push(format!("[OK] {} exists: {}", label, path.display()));
        } else {
            report.ok = false;
            report
                .messages
                .push(format!("[FAIL] {} missing: {}", label, path.display()));
        }
    }

    #[cfg(unix)]
    check_unix_integration(paths, &mut report)?;

    #[cfg(windows)]
    check_windows_integration(paths, &mut report)?;

    Ok(report)
}

#[cfg(unix)]
fn get_installed_shells() -> io::Result<Vec<String>> {
    const SHELLS_FILE: &str = "/etc/shells";
    if !Path::new(SHELLS_FILE).exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(SHELLS_FILE)?;
    let shells = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect();
    Ok(shells)
}

#[cfg(windows)]
fn add_to_windows_path(paths: &UpstreamPaths) -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to open registry key: {}", e),
            )
        })?;

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
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to set PATH: {}", e)))?;

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
    fs::create_dir_all(&paths.dirs.metadata_dir)?;
    fs::create_dir_all(&paths.install.appimages_dir)?;
    fs::create_dir_all(&paths.install.binaries_dir)?;
    fs::create_dir_all(&paths.install.archives_dir)?;
    fs::create_dir_all(&paths.integration.icons_dir)?;
    fs::create_dir_all(&paths.integration.symlinks_dir)?;
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
    Ok(())
}

#[cfg(windows)]
fn create_metadata_files(_paths: &UpstreamPaths) -> io::Result<()> {
    // On Windows, we use registry-based PATH, so no metadata files needed
    Ok(())
}

#[cfg(unix)]
fn update_shell_profiles(paths: &UpstreamPaths) -> io::Result<()> {
    let shells = get_installed_shells()?;
    for shell_path in shells {
        let shell_name = Path::new(&shell_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        match shell_name.to_lowercase().as_str() {
            "bash" | "sh" => {
                add_line_to_profile(paths, ".bashrc", SOURCE_LINE_BASH)?;
            }
            "zsh" => {
                add_line_to_profile(paths, ".zshrc", SOURCE_LINE_BASH)?;
            }
            "fish" => {
                let fish_config = Path::new(".config").join("fish").join("config.fish");
                add_line_to_profile(paths, fish_config.to_str().unwrap(), SOURCE_LINE_FISH)?;
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
        report.ok = false;
        report.messages.push(format!(
            "[FAIL] PATH metadata file missing: {}",
            paths.config.paths_file.display()
        ));
    } else {
        let content = fs::read_to_string(&paths.config.paths_file)?;
        if content.contains(&expected_line) {
            report.messages.push(format!(
                "[OK] PATH metadata file contains symlink export: {}",
                paths.config.paths_file.display()
            ));
        } else {
            report.ok = false;
            report.messages.push(format!(
                "[FAIL] PATH metadata file missing expected export line: {}",
                paths.config.paths_file.display()
            ));
        }
    }

    let mut profiles_to_check: BTreeSet<(String, String)> = BTreeSet::new();
    for shell_path in get_installed_shells()? {
        let shell_name = Path::new(&shell_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match shell_name.as_str() {
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
            _ => {}
        }
    }

    for (profile_rel, expected_line) in profiles_to_check {
        let profile_path = paths.dirs.user_dir.join(&profile_rel);
        if !profile_path.exists() {
            report.ok = false;
            report.messages.push(format!(
                "[FAIL] Shell profile missing: {}",
                profile_path.display()
            ));
            continue;
        }

        let content = fs::read_to_string(&profile_path)?;
        if content.contains(&expected_line) {
            report.messages.push(format!(
                "[OK] Shell profile contains upstream hook: {}",
                profile_path.display()
            ));
        } else {
            report.ok = false;
            report.messages.push(format!(
                "[FAIL] Shell profile missing upstream hook: {}",
                profile_path.display()
            ));
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
pub fn cleanup(paths: &UpstreamPaths) -> io::Result<()> {
    let shells = get_installed_shells()?;
    for shell_path in shells {
        let shell_name = Path::new(&shell_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let profile = match shell_name.to_lowercase().as_str() {
            "bash" | "sh" => Some(".bashrc"),
            "zsh" => Some(".zshrc"),
            "fish" => Some(".config/fish/config.fish"),
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
                .replace(SOURCE_LINE_FISH, "");
            fs::write(&profile_path, content)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn cleanup(paths: &UpstreamPaths) -> io::Result<()> {
    remove_from_windows_path(paths)
}

#[cfg(windows)]
fn remove_from_windows_path(paths: &UpstreamPaths) -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to open registry key: {}", e),
            )
        })?;

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
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to set PATH: {}", e)))?;

    // Broadcast WM_SETTINGCHANGE to notify other applications
    broadcast_environment_change();

    Ok(())
}

#[cfg(windows)]
fn check_windows_integration(
    paths: &UpstreamPaths,
    report: &mut InitCheckReport,
) -> io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu
        .open_subkey_with_flags("Environment", KEY_READ)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to open PATH: {}", e)))?;

    let symlinks_path = paths.integration.symlinks_dir.display().to_string();
    let symlinks_norm = normalize_windows_path(&symlinks_path);
    let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

    let in_path = current_path
        .split(';')
        .any(|p| normalize_windows_path(p) == symlinks_norm);

    if in_path {
        report
            .messages
            .push("[OK] Windows PATH contains upstream symlinks directory".to_string());
    } else {
        report.ok = false;
        report
            .messages
            .push("[FAIL] Windows PATH missing upstream symlinks directory".to_string());
    }

    Ok(())
}
