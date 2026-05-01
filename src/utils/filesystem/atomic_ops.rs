use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};

/// Atomically replace a file's contents by writing to a temp file in the same
/// directory, syncing it, renaming it into place, and syncing the directory.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Path '{}' has no parent directory", path.display()))?;

    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_path = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("atomic"),
        std::process::id(),
        unique
    ));

    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .with_context(|| format!("Failed to create temp file '{}'", temp_path.display()))?;

    temp_file
        .write_all(bytes)
        .with_context(|| format!("Failed to write temp file '{}'", temp_path.display()))?;
    temp_file
        .sync_all()
        .with_context(|| format!("Failed to sync temp file '{}'", temp_path.display()))?;
    drop(temp_file);

    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to atomically replace '{}' via '{}'",
            path.display(),
            temp_path.display()
        )
    })?;

    sync_parent_directory(parent)
        .with_context(|| format!("Failed to sync directory '{}'", parent.display()))?;

    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> Result<()> {
    let dir = OpenOptions::new()
        .read(true)
        .open(parent)
        .with_context(|| format!("Failed to open directory '{}'", parent.display()))?;
    dir.sync_all()
        .with_context(|| format!("Failed to sync directory '{}'", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> Result<()> {
    Ok(())
}
