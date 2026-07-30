use std::{fs, io, path::Path};

use anyhow::{Context, Result};

pub mod atomic_ops;
pub mod manifest_sync;
pub mod safe_move;

pub fn path_exists_no_follow(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to inspect path '{}'", path.display()))
        }
    }
}
