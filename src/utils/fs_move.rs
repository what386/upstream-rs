use anyhow::{Context, Result};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Move a file or directory, transparently falling back to copy+delete when
/// the source and destination are on different filesystems.
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

/// Check whether an IO error represents a cross-device rename failure.
pub fn is_cross_device(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::CrossesDevices
}

/// Copy the source to destination and remove the source when rename cannot be used.
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
#[path = "../../tests/utils/fs_move.rs"]
mod tests;
