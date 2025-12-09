use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result};

use crate::models::upstream::Package;

/// ------------------------------------------
/// Manages symbolic links for binary aliases.
/// ------------------------------------------

/// Creates a symbolic link in the binaries directory pointing to the target file.
pub fn add_link(package: &Package, symlink_dir: &Path) -> Result<()> {
    let exec_path = package.exec_path
        .as_ref()
        .context("Exec path not set")?;

    if !Path::new(exec_path).exists() {
        anyhow::bail!("Target file not found: {}", exec_path);
    }

    let symlink = symlink_dir.join(&package.name);

    // Remove existing symlink if present
    if symlink.exists() {
        fs::remove_file(&symlink)
            .context("Failed to remove existing symlink")?;
    }

    create_symlink(exec_path, &symlink)?;

    Ok(())
}

/// Removes a symbolic link by its package name.
pub fn remove_link(package: &Package, symlink_dir: &Path) -> Result<()> {
    let symlink = symlink_dir.join(&package.name);

    if symlink.exists() {
        fs::remove_file(&symlink)
            .context("Failed to remove symlink")?;
    }

    Ok(())
}

pub fn get_symlink(package: &Package, symlink_dir: &Path) -> Option<PathBuf> {
    let symlink = symlink_dir.join(&package.name);

    if symlink.exists() {
        Some(symlink)
    } else {
        None
    }
}

fn create_symlink(target_path: &str, symlink: &Path) -> Result<()> {
    let output = Command::new("ln")
        .arg("-s")
        .arg(target_path)
        .arg(symlink)
        .output()
        .context("Failed to execute ln")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create symlink: {}", stderr);
    }

    Ok(())
}
