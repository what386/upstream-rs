use anyhow::{Context, Result, anyhow};
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
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

const STALE_LOCK_MAX_AGE_SECS: u64 = 60 * 60 * 24;

#[derive(Default, Debug)]
struct LockMetadata {
    pid: Option<u32>,
    operation: Option<String>,
    started_at_unix: Option<u64>,
}

impl LockStorage {
    pub fn acquire(paths: &UpstreamPaths, command: &Commands) -> Result<Self> {
        let lock_path = paths.dirs.metadata_dir.join("lock");
        let operation = command.to_string();
        Self::acquire_at(&lock_path, &operation)
    }

    fn acquire_at(lock_path: &Path, operation: &str) -> Result<Self> {
        Self::acquire_at_internal(lock_path, operation, true)
    }

    fn acquire_at_internal(lock_path: &Path, operation: &str, allow_recovery: bool) -> Result<Self> {
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

                if allow_recovery && Self::is_stale_lock(&lock_info) {
                    match fs::remove_file(lock_path) {
                        Ok(_) => {
                            return Self::acquire_at_internal(lock_path, operation, false);
                        }
                        Err(remove_err) if remove_err.kind() == ErrorKind::NotFound => {
                            return Self::acquire_at_internal(lock_path, operation, false);
                        }
                        Err(remove_err) => {
                            return Err(remove_err).context(format!(
                                "Lock at '{}' appears stale but could not be removed",
                                lock_path.display()
                            ));
                        }
                    }
                }

                let meta = Self::parse_lock_metadata(&lock_info);
                return Err(anyhow!(
                    "Another upstream operation is already running.\n\
                     Lock file: {}\n\
                     Holder info: {}\n\
                     If this looks stale, remove the lock file and retry.\n\
                     parsed_pid={:?}, parsed_operation={:?}, parsed_started_at_unix={:?}",
                    lock_path.display(),
                    lock_info.trim(),
                    meta.pid,
                    meta.operation,
                    meta.started_at_unix
                ));
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("Failed to create lock file '{}'", lock_path.display())
                });
            }
        };

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

    fn parse_lock_metadata(lock_info: &str) -> LockMetadata {
        let mut meta = LockMetadata::default();
        for raw_line in lock_info.lines() {
            let line = raw_line.trim();
            if let Some(value) = line.strip_prefix("pid=") {
                meta.pid = value.trim().parse::<u32>().ok();
            } else if let Some(value) = line.strip_prefix("operation=") {
                let op = value.trim();
                if !op.is_empty() {
                    meta.operation = Some(op.to_string());
                }
            } else if let Some(value) = line.strip_prefix("started_at_unix=") {
                meta.started_at_unix = value.trim().parse::<u64>().ok();
            }
        }
        meta
    }

    fn is_stale_lock(lock_info: &str) -> bool {
        let meta = Self::parse_lock_metadata(lock_info);

        if let Some(pid) = meta.pid
            && !Self::process_exists(pid)
        {
            return true;
        }

        if let Some(started_at) = meta.started_at_unix {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(started_at);
            if now.saturating_sub(started_at) > STALE_LOCK_MAX_AGE_SECS {
                return true;
            }
        }

        false
    }

    fn process_exists(pid: u32) -> bool {
        #[cfg(unix)]
        {
            return Path::new("/proc").join(pid.to_string()).exists();
        }

        #[cfg(not(unix))]
        {
            let _ = pid;
            true
        }
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
