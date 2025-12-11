use std::{fs, path::PathBuf};
use std::path::Path;
use std::os::unix::fs::PermissionsExt;
use anyhow::{Context, Result};

/// Sets executable permissions on a file for user, group, and others.
pub fn make_executable(exec_path: &Path) -> Result<()> {

    if !exec_path.exists() {
        anyhow::bail!("Invalid executable path: {}", exec_path.to_string_lossy());
    }

    match fs::metadata(exec_path) {
        Ok(metadata) => {
            let mut permissions = metadata.permissions();
            let mode = permissions.mode();

            permissions.set_mode(mode | 0o111);

            fs::set_permissions(exec_path, permissions)
                .context("Failed to set executable permissions")?;
        }
        Err(e) => {
            return Err(e).context("Failed to read metadata");
        }
    }

    Ok(())
}

/// Finds any potential executables in a directory.
pub fn find_executable(directory_path: &Path, name: &str) -> Option<PathBuf> {
    // 1. bin/<name>
    let bin_path = directory_path.join("bin").join(name);
    if bin_path.is_file() {
        return Some(bin_path);
    }

    // 2. directoryPath/<name>
    let direct_path = directory_path.join(name);
    if direct_path.is_file() {
        return Some(direct_path);
    }

    // 3. directory name is the executable name
    //    e.g. cool-app-x86_64/cool-app-x86_64
    if let Some(dir_name) = directory_path.file_name() {
        let derived_path = directory_path.join(dir_name);
        if derived_path.is_file() {
            return Some(derived_path);
        }
    }

    // 4. As a fallback, search for any file starting with name
    //    e.g. "cool-app" -> "cool-app-x86_64", "cool-app-v1"
    if let Ok(entries) = fs::read_dir(directory_path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(file_name) = entry.file_name().to_str() {
                        if file_name.to_lowercase().starts_with(&name.to_lowercase()) {
                            return Some(entry.path());
                        }
                    }
                }
            }
        }
    }

    None
}
