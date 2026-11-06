#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
use std::fs;
use std::path::Path;

/// Sets executable permissions on a file for user, group, and others.
#[cfg(unix)]
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

#[cfg(windows)]
pub fn make_executable(_exec_path: &Path) -> Result<()> {
    Ok(())
}
