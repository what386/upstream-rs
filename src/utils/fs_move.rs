use anyhow::{Context, Result};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn move_file_or_dir(src: &Path, dst: &Path) -> Result<()> {
    move_file_or_dir_with_rename(src, dst, |from, to| fs::rename(from, to))
}

fn move_file_or_dir_with_rename<F>(src: &Path, dst: &Path, mut rename_fn: F) -> Result<()>
where
    F: FnMut(&Path, &Path) -> io::Result<()>,
{
    match rename_fn(src, dst) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device(&err) => fallback_move(src, dst),
        Err(err) => Err(err).context(format!(
            "Failed to move '{}' to '{}'",
            src.display(),
            dst.display()
        )),
    }
}

pub fn is_cross_device(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::CrossesDevices
}

fn fallback_move(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::metadata(src)
        .with_context(|| format!("Failed to read metadata for '{}'", src.display()))?;

    if metadata.is_dir() {
        copy_dir_recursive(src, dst)
            .with_context(|| format!("Failed to copy directory to '{}'", dst.display()))?;
        fs::remove_dir_all(src)
            .with_context(|| format!("Failed to remove source directory '{}'", src.display()))?;
        return Ok(());
    }

    fs::copy(src, dst).with_context(|| {
        format!(
            "Failed to copy file from '{}' to '{}'",
            src.display(),
            dst.display()
        )
    })?;
    fs::set_permissions(dst, metadata.permissions())
        .with_context(|| format!("Failed to preserve file permissions on '{}'", dst.display()))?;
    fs::remove_file(src)
        .with_context(|| format!("Failed to remove source file '{}'", src.display()))?;
    Ok(())
}

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

fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    let link_target = fs::read_link(src)?;

    #[cfg(unix)]
    {
        return std::os::unix::fs::symlink(link_target, dst);
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
    use super::{is_cross_device, move_file_or_dir, move_file_or_dir_with_rename};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

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
    fn cross_device_error_detection_matches_error_kind() {
        let err = io::Error::new(io::ErrorKind::CrossesDevices, "cross-device");
        assert!(is_cross_device(&err));
    }

    #[test]
    fn fallback_move_copies_and_removes_source_file() {
        let root = temp_root("fallback-file");
        fs::create_dir_all(&root).expect("create root");
        let src = root.join("source.txt");
        let dst = root.join("dest.txt");
        fs::write(&src, b"hello").expect("write source");

        move_file_or_dir_with_rename(&src, &dst, |_, _| {
            Err(io::Error::new(io::ErrorKind::CrossesDevices, "xdev"))
        })
        .expect("fallback move");

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

        move_file_or_dir_with_rename(&src, &dst, |_, _| {
            Err(io::Error::new(io::ErrorKind::CrossesDevices, "xdev"))
        })
        .expect("fallback dir move");

        assert!(!src.exists());
        assert_eq!(
            fs::read(dst.join("nested/file.txt")).expect("read moved file"),
            b"nested-data"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
