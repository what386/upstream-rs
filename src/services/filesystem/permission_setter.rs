use std::fs;
use std::path::Path;
use std::os::unix::fs::PermissionsExt;
use anyhow::{Context, Result};
use crate::models::upstream::Package;

/// ----------------------------------------------------------------------------------
/// Manages file permissions, specifically setting executable permissions on binaries.
/// ----------------------------------------------------------------------------------

/// Sets executable permissions on a file for user, group, and others.
pub fn make_executable(package: &Package) -> Result<()> {
    let exec_path = package.exec_path
        .as_ref()
        .context("Exec path not set")?;

    let path = Path::new(exec_path);
    if !path.exists() {
        anyhow::bail!("Invalid package installation: {}", package.name);
    }

    match fs::metadata(path) {
        Ok(metadata) => {
            let mut permissions = metadata.permissions();
            let mode = permissions.mode();

            permissions.set_mode(mode | 0o111);

            fs::set_permissions(path, permissions)
                .context("Failed to set executable permissions")?;
        }
        Err(e) => {
            return Err(e).context("Failed to read metadata");
        }
    }

    Ok(())
}

/// Finds any potential executables in a directory.
pub fn find_executable(directory_path: &str, name: &str) -> Option<String> {
    let dir = Path::new(directory_path);

    // 1. bin/<name>
    let bin_path = dir.join("bin").join(name);
    if bin_path.is_file() {
        return Some(bin_path.to_string_lossy().to_string());
    }

    // 2. directoryPath/<name>
    let direct_path = dir.join(name);
    if direct_path.is_file() {
        return Some(direct_path.to_string_lossy().to_string());
    }

    // 3. directory name is the executable name
    //    e.g. cool-app-x86_64/cool-app-x86_64
    if let Some(dir_name) = dir.file_name() {
        let derived_path = dir.join(dir_name);
        if derived_path.is_file() {
            return Some(derived_path.to_string_lossy().to_string());
        }
    }

    // 4. As a fallback, search for any file starting with name
    //    e.g. "cool-app" -> "cool-app-x86_64", "cool-app-v1"
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.to_lowercase().starts_with(&name.to_lowercase()) {
                            return Some(entry.path().to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    None
}
