use std::env;
use std::fs;
use std::path::{Path};
use anyhow::{Context, Result};

use crate::models::upstream::Package;

/// Creates or updates the `path.sh` file, ensuring it contains the
/// global symlinks directory and a standard header.
pub fn initialize(paths_file: &Path) -> Result<()> {
    const HEADER: &str = "#!/bin/bash\n# Upstream managed PATH additions\n";

    let username = env::var("USER").unwrap_or_else(|_| "user".to_string());
    let path_line = format!("export PATH=\"/home/{username}/.upstream/symlinks:$PATH\"");

    // Create if missing
    if !paths_file.exists() {
        let content = format!("{HEADER}{path_line}\n");
        fs::write(paths_file, content)
            .context("Failed to create paths file")?;
        return Ok(());
    }

    // Ensure the line exists exactly once
    let existing = fs::read_to_string(paths_file)
        .context("Failed to read paths file")?;

    if !existing.contains(&path_line) {
        let updated = format!("{existing}{path_line}\n");
        fs::write(paths_file, updated)
            .context("Failed to update paths file")?;
    }

    Ok(())
}

/// Adds a package’s installation path to PATH by appending an export line.
pub fn add_to_paths(package: &Package, paths_file: &Path) -> Result<String> {
    let install_path = package.install_path
        .as_ref()
        .context("Install path not set")?;

    if !Path::new(install_path).is_dir() {
        anyhow::bail!("Package install directory not found: {}", install_path);
    }

    let mut content = fs::read_to_string(paths_file)
        .context("Failed to read paths file")?;

    // Escape characters for shell safety
    let escaped = install_path
        .replace("$", "\\$")
        .replace('"', "\\\"");

    let export_line = format!("export PATH=\"{escaped}:$PATH\"");

    if !content.contains(&export_line) {
        content.push_str(&format!("{export_line}\n"));
        fs::write(paths_file, &content)
            .context("Failed to write paths file")?;
    }

    Ok(export_line)
}

/// Removes a package’s PATH entry from `path.sh`.
pub fn remove_from_paths(package: &Package, paths_file: &Path) -> Result<()> {
    let install_path = package.install_path
        .as_ref()
        .context("Install path not set")?;

    let mut content = fs::read_to_string(paths_file)
        .context("Failed to read paths file")?;

    let escaped = install_path
        .replace("$", "\\$")
        .replace('"', "\\\"");

    let export_line = format!("export PATH=\"{escaped}:$PATH\"");

    // Remove with or without trailing newline
    content = content.replace(&(export_line.clone() + "\n"), "");
    content = content.replace(&export_line, "");

    fs::write(paths_file, content)
        .context("Failed to write paths file")?;

    Ok(())
}
