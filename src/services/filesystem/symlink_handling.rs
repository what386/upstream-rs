use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

use crate::utils::upstream_paths::PATHS;

/// Creates a symbolic link in the binaries directory pointing to the target file.
pub fn add_link(exec_path: &Path, name: &str) -> Result<()> {
    if !Path::new(exec_path).exists() {
        anyhow::bail!("Target file not found");
    }

    let symlink = PATHS.symlinks_dir.join(name);

    // Remove existing symlink if present
    if symlink.exists() {
        fs::remove_file(&symlink)
            .context("Failed to remove existing symlink")?;
    }

    create_symlink(exec_path, &symlink)?;

    Ok(())
}

/// Removes a symbolic link by its package name.
pub fn remove_link(name: &str) -> Result<()> {
    let symlink = PATHS.symlinks_dir.join(name);

    if symlink.exists() {
        fs::remove_file(&symlink)
            .context("Failed to remove symlink")?;
    }

    Ok(())
}

/// Gets the path to a symlink if it exists.
pub fn get_symlink(name: &str) -> Option<PathBuf> {
    let symlink = PATHS.symlinks_dir.join(name);

    // Simpler: just check exists() and return in one expression
    symlink.exists().then_some(symlink)
}

#[cfg(unix)]
fn create_symlink(target_path: &Path, symlink: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target_path, symlink)
        .context("Failed to create symlink")
}

// may be unused: windows support is in question
#[cfg(windows)]
fn create_symlink(target_path: &str, symlink: &Path) -> Result<()> {
    std::os::windows::fs::symlink_file(target_path, symlink)
        .context("Failed to create symlink")
}
