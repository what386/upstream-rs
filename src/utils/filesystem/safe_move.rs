use anyhow::{Context, Result};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::{thread, time::Duration};

#[cfg(windows)]
const WINDOWS_RENAME_RETRIES: usize = 20;
#[cfg(windows)]
const WINDOWS_RENAME_RETRY_DELAY: Duration = Duration::from_millis(50);

/// Move a file or directory, falling back to copy+delete
/// if the source and dest are on different filesystems.
pub fn move_file_or_dir(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(windows)]
    let rename_result = rename_with_retry(src, dst);
    #[cfg(not(windows))]
    let rename_result = fs::rename(src, dst);

    match rename_result {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::CrossesDevices => move_via_copy(src, dst),
        Err(err) => Err(err).context(format!(
            "Failed to move '{}' to '{}'",
            src.display(),
            dst.display()
        )),
    }
}

#[cfg(windows)]
fn rename_with_retry(src: &Path, dst: &Path) -> io::Result<()> {
    for attempt in 0..=WINDOWS_RENAME_RETRIES {
        match fs::rename(src, dst) {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < WINDOWS_RENAME_RETRIES && is_retryable_windows_rename(&error) =>
            {
                thread::sleep(WINDOWS_RENAME_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("bounded rename loop always returns")
}

#[cfg(windows)]
fn is_retryable_windows_rename(error: &io::Error) -> bool {
    use winapi::shared::winerror::{
        ERROR_ACCESS_DENIED, ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION,
    };

    matches!(
        error.raw_os_error(),
        Some(code)
            if code == ERROR_ACCESS_DENIED as i32
                || code == ERROR_SHARING_VIOLATION as i32
                || code == ERROR_LOCK_VIOLATION as i32
    )
}

pub fn copy_file_or_dir(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::metadata(src)
        .with_context(|| format!("Failed to read metadata for '{}'", src.display()))?;
    if metadata.is_dir() {
        copy_dir_recursive(src, dst)
            .with_context(|| format!("Failed to copy directory to '{}'", dst.display()))
    } else {
        fs::copy(src, dst).with_context(|| {
            format!(
                "Failed to copy file from '{}' to '{}'",
                src.display(),
                dst.display()
            )
        })?;
        fs::set_permissions(dst, metadata.permissions())
            .with_context(|| format!("Failed to preserve file permissions on '{}'", dst.display()))
    }
}

/// Copy the source to destination and remove the source when rename cannot be used.
fn move_via_copy(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::metadata(src)
        .with_context(|| format!("Failed to read metadata for '{}'", src.display()))?;

    if metadata.is_dir() {
        copy_file_or_dir(src, dst)?;
        fs::remove_dir_all(src)
            .with_context(|| format!("Failed to remove source directory '{}'", src.display()))?;
        return Ok(());
    }

    copy_file_or_dir(src, dst)?;
    fs::remove_file(src)
        .with_context(|| format!("Failed to remove source file '{}'", src.display()))?;
    Ok(())
}

/// Recursively copy a directory while preserving permissions and symlinks.
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("Destination already exists: '{}'", dst.display()),
        ));
    }

    fs::create_dir_all(dst)?;
    let src_metadata = fs::metadata(src)?;
    fs::set_permissions(dst, src_metadata.permissions())?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path: PathBuf = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_recursive(&entry_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&entry_path, &target_path)?;
            let source_permissions = fs::metadata(&entry_path)?.permissions();
            fs::set_permissions(&target_path, source_permissions)?;
        } else if file_type.is_symlink() {
            copy_symlink(&entry_path, &target_path)?;
        } else {
            return Err(io::Error::other(format!(
                "Unsupported entry type while moving directory: '{}'",
                entry_path.display()
            )));
        }
    }

    Ok(())
}

/// Recreate a symlink at `dst` with the same link target as `src`.
fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    let link_target = fs::read_link(src)?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(link_target, dst)
    }

    #[cfg(windows)]
    {
        if src.metadata()?.is_dir() {
            return std::os::windows::fs::symlink_dir(link_target, dst);
        }
        return std::os::windows::fs::symlink_file(link_target, dst);
    }
}

#[cfg(test)]
mod tests {
    use super::{move_file_or_dir, move_via_copy};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-fs-move-test-{name}-{nanos}"))
    }

    #[test]
    fn move_file_or_dir_moves_file_with_rename_path() {
        let root = temp_root("rename");
        fs::create_dir_all(&root).expect("create root");
        let src = root.join("source.bin");
        let dst = root.join("dest.bin");
        fs::write(&src, b"content").expect("write source");

        move_file_or_dir(&src, &dst).expect("rename move");

        assert!(!src.exists());
        assert_eq!(fs::read(&dst).expect("read destination"), b"content");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn fallback_move_copies_and_removes_source_file() {
        let root = temp_root("fallback-file");
        fs::create_dir_all(&root).expect("create root");
        let src = root.join("source.txt");
        let dst = root.join("dest.txt");
        fs::write(&src, b"hello").expect("write source");

        move_via_copy(&src, &dst).expect("fallback move");

        assert!(!src.exists());
        assert_eq!(fs::read(&dst).expect("read destination"), b"hello");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn fallback_move_handles_directories_recursively() {
        let root = temp_root("fallback-dir");
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(src.join("nested")).expect("create nested src");
        fs::write(src.join("nested/file.txt"), b"nested-data").expect("write nested file");

        move_via_copy(&src, &dst).expect("fallback dir move");

        assert!(!src.exists());
        assert_eq!(
            fs::read(dst.join("nested/file.txt")).expect("read moved file"),
            b"nested-data"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(windows)]
    #[test]
    fn windows_can_replace_a_running_executable() {
        const MODE_ENV: &str = "UPSTREAM_SAFE_MOVE_CHILD_MODE";
        const READY_ENV: &str = "UPSTREAM_SAFE_MOVE_READY_FILE";
        const TEST_NAME: &str =
            "utils::filesystem::safe_move::tests::windows_can_replace_a_running_executable";

        if std::env::var_os(MODE_ENV).as_deref() == Some(std::ffi::OsStr::new("hold")) {
            let ready = std::env::var_os(READY_ENV).expect("child ready path");
            fs::write(ready, b"ready").expect("signal child readiness");
            std::thread::sleep(std::time::Duration::from_secs(2));
            return;
        }

        let root = temp_root("running-executable");
        fs::create_dir_all(&root).expect("create root");
        let source = root.join("upstream.exe");
        let backup = root.join("upstream.old.exe");
        let ready = root.join("ready");
        let test_binary = std::env::current_exe().expect("resolve test binary");
        fs::copy(&test_binary, &source).expect("copy source executable");

        let mut child = std::process::Command::new(&source)
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(MODE_ENV, "hold")
            .env(READY_ENV, &ready)
            .spawn()
            .expect("start executable through alias");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ready.is_file(), "child did not become ready");

        move_file_or_dir(&source, &backup).expect("rename running executable");
        fs::write(&source, b"replacement").expect("install replacement executable");

        assert_eq!(fs::read(&source).expect("read replacement"), b"replacement");
        assert!(backup.is_file());
        assert!(child.wait().expect("wait for old process").success());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(windows)]
    #[test]
    fn windows_retryable_rename_errors_are_limited_to_lock_failures() {
        use super::is_retryable_windows_rename;
        use winapi::shared::winerror::{
            ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_LOCK_VIOLATION,
            ERROR_SHARING_VIOLATION,
        };

        for code in [
            ERROR_ACCESS_DENIED,
            ERROR_SHARING_VIOLATION,
            ERROR_LOCK_VIOLATION,
        ] {
            assert!(is_retryable_windows_rename(
                &std::io::Error::from_raw_os_error(code as i32)
            ));
        }
        assert!(!is_retryable_windows_rename(
            &std::io::Error::from_raw_os_error(ERROR_FILE_NOT_FOUND as i32)
        ));
    }
}
