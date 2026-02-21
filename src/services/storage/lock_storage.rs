use anyhow::{Context, Result, anyhow};
use std::{
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::application::cli::arguments::Commands;
use crate::utils::static_paths::UpstreamPaths;

#[derive(Debug)]
pub struct LockStorage {
    path: PathBuf,
}

impl LockStorage {
    pub fn acquire(paths: &UpstreamPaths, command: &Commands) -> Result<Self> {
        let lock_path = paths.dirs.metadata_dir.join("lock");
        let operation = command.to_string();
        Self::acquire_at(&lock_path, &operation)
    }

    fn acquire_at(lock_path: &Path, operation: &str) -> Result<Self> {
        let lock_parent = lock_path
            .parent()
            .ok_or_else(|| anyhow!("Invalid lock path '{}'", lock_path.display()))?;

        fs::create_dir_all(lock_parent).with_context(|| {
            format!(
                "Failed to create lock directory '{}'",
                lock_parent.display()
            )
        })?;

        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                let lock_info = fs::read_to_string(lock_path)
                    .unwrap_or_else(|_| "<lock details unavailable>".to_string());
                return Err(anyhow!(
                    "Another upstream operation is already running.\n\
                     Lock file: {}\n\
                     Holder info: {}\n\
                     If this looks stale, remove the lock file and retry.",
                    lock_path.display(),
                    lock_info.trim()
                ));
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("Failed to create lock file '{}'", lock_path.display())
                });
            }
        };

        use std::io::Write;
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        writeln!(file, "pid={}", process::id()).ok();
        writeln!(file, "operation={}", operation).ok();
        writeln!(file, "started_at_unix={}", since_epoch).ok();

        Ok(Self {
            path: lock_path.to_path_buf(),
        })
    }
}

impl Drop for LockStorage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "../../../tests/services/storage/lock_storage.rs"]
mod tests;
