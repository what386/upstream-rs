use anyhow::{Context, Result, anyhow};
#[cfg(unix)]
use std::process::Command;
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process, thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::application::cli::arguments::Commands;
use crate::utils::static_paths::UpstreamPaths;

#[derive(Debug)]
pub struct LockStorage {
    path: PathBuf,
}

const STALE_LOCK_MAX_AGE_SECS: Duration = Duration::from_mins(45);
const LOCK_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default, Debug)]
struct LockMetadata {
    pid: Option<u32>,
    operation: Option<String>,
    started_at_unix: Option<u64>,
}

enum AcquireOutcome {
    Acquired(LockStorage),
    Waiting,
}

impl LockStorage {
    pub fn acquire(paths: &UpstreamPaths, command: &Commands) -> Result<Self> {
        let lock_path = paths.dirs.metadata_dir.join("lock");
        let operation = command.to_string();
        Self::acquire_at(&lock_path, &operation)
    }

    fn acquire_at(lock_path: &Path, operation: &str) -> Result<Self> {
        let mut printed_wait_notice = false;

        loop {
            match Self::try_acquire_at_internal(lock_path, operation, true)? {
                AcquireOutcome::Acquired(lock) => return Ok(lock),
                AcquireOutcome::Waiting => {
                    if !printed_wait_notice {
                        eprintln!("Waiting for lock file...");
                        printed_wait_notice = true;
                    }
                    thread::sleep(LOCK_POLL_INTERVAL);
                }
            }
        }
    }

    fn try_acquire_at_internal(
        lock_path: &Path,
        operation: &str,
        allow_recovery: bool,
    ) -> Result<AcquireOutcome> {
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
                            return Self::try_acquire_at_internal(lock_path, operation, false);
                        }
                        Err(remove_err) if remove_err.kind() == ErrorKind::NotFound => {
                            return Self::try_acquire_at_internal(lock_path, operation, false);
                        }
                        Err(remove_err) => {
                            return Err(remove_err).context(format!(
                                "Lock at '{}' appears stale but could not be removed",
                                lock_path.display()
                            ));
                        }
                    }
                }

                return Ok(AcquireOutcome::Waiting);
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

        Ok(AcquireOutcome::Acquired(Self {
            path: lock_path.to_path_buf(),
        }))
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
            if Duration::from_secs(now.saturating_sub(started_at)) > STALE_LOCK_MAX_AGE_SECS {
                return true;
            }
        }

        false
    }

    fn process_exists(pid: u32) -> bool {
        if pid == 0 {
            return false;
        }

        #[cfg(unix)]
        {
            // Linux: /proc is cheap and reliable.
            if Path::new("/proc").exists() {
                return Path::new("/proc").join(pid.to_string()).exists();
            }

            // macOS/BSD fallback: `kill -0 <pid>` checks whether the process exists.
            // If the probe command is unavailable, avoid false stale detection.
            Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .status()
                .map(|status| status.success())
                .unwrap_or(true)
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
mod tests {
    use super::LockStorage;
    use std::{
        fs,
        path::PathBuf,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn unique_lock_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-lock-test-{name}-{nanos}"))
            .join("metadata")
            .join("lock")
    }

    #[test]
    fn lock_waits_for_concurrent_acquire_to_finish() {
        let lock_path = unique_lock_path("concurrent");
        let guard = LockStorage::acquire_at(&lock_path, "test").expect("first lock");
        let release_path = lock_path.clone();

        let releaser = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            drop(guard);
        });

        let _guard = LockStorage::acquire_at(&lock_path, "test").expect("lock after wait");
        releaser.join().expect("join releaser");

        let _ = fs::remove_dir_all(release_path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn lock_releases_on_drop() {
        let lock_path = unique_lock_path("release");
        {
            let _guard = LockStorage::acquire_at(&lock_path, "test").expect("first lock");
        }

        let _guard2 = LockStorage::acquire_at(&lock_path, "test").expect("lock after drop");

        let _ = fs::remove_dir_all(lock_path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn stale_lock_is_recovered_automatically() {
        let lock_path = unique_lock_path("stale-recover");
        fs::create_dir_all(lock_path.parent().expect("lock parent")).expect("create lock parent");
        // Deliberately invalid/non-existent pid with old start time.
        fs::write(
            &lock_path,
            "pid=999999\noperation=test\nstarted_at_unix=1\n",
        )
        .expect("write stale lock");

        let _guard = LockStorage::acquire_at(&lock_path, "new-op").expect("recover stale lock");
        let contents = fs::read_to_string(&lock_path).expect("read lock");
        assert!(contents.contains("operation=new-op"));

        let _ = fs::remove_dir_all(lock_path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn parse_lock_metadata_extracts_known_fields() {
        let meta = LockStorage::parse_lock_metadata(
            "pid=123\noperation=upgrade\nstarted_at_unix=456\nunknown=ignored\n",
        );
        assert_eq!(meta.pid, Some(123));
        assert_eq!(meta.operation.as_deref(), Some("upgrade"));
        assert_eq!(meta.started_at_unix, Some(456));
    }

    #[test]
    fn active_lock_still_blocks_second_acquire() {
        let lock_path = unique_lock_path("active-block");
        fs::create_dir_all(lock_path.parent().expect("lock parent")).expect("create lock parent");
        let current_pid = std::process::id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        fs::write(
            &lock_path,
            format!("pid={current_pid}\noperation=test\nstarted_at_unix={now}\n"),
        )
        .expect("write active lock");

        let outcome =
            LockStorage::try_acquire_at_internal(&lock_path, "next-op", true).expect("try acquire");
        assert!(matches!(outcome, super::AcquireOutcome::Waiting));

        let _ = fs::remove_dir_all(lock_path.parent().unwrap().parent().unwrap());
    }
}
