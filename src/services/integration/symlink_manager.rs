use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;

use crate::utils::filesystem::path_exists_no_follow;

#[cfg(windows)]
use std::ffi::OsStr;

pub struct SymlinkManager<'a> {
    symlinks_dir: &'a Path,
}

impl<'a> SymlinkManager<'a> {
    fn remove_link_path(path: &Path, context_message: &'static str) -> Result<()> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.is_dir() && !metadata.file_type().is_symlink() {
                    anyhow::bail!(
                        "Refusing to remove directory at '{}' while managing symlink",
                        path.display()
                    );
                }
                fs::remove_file(path).context(context_message)?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).context(context_message),
        }

        Ok(())
    }

    fn platform_link_path(link: &Path) -> std::path::PathBuf {
        #[cfg(windows)]
        {
            if link.extension() != Some(OsStr::new("exe")) {
                return link.with_extension("exe");
            }
        }

        link.to_path_buf()
    }

    pub fn new(symlinks_dir: &'a Path) -> Self {
        Self { symlinks_dir }
    }

    /// Creates a symbolic link in the symlinks directory pointing to the target file
    pub fn add_link(&self, exec_path: &Path, name: &str) -> Result<()> {
        if !exec_path.exists() {
            anyhow::bail!("Target file not found: {}", exec_path.display());
        }

        let base_link = self.symlinks_dir.join(name);
        let symlink = Self::platform_link_path(&base_link);

        // Remove existing link if present.
        Self::remove_link_path(&symlink, "Failed to remove existing symlink")?;
        // Cleanup stale pre-fix path variant on Windows.
        if base_link != symlink {
            Self::remove_link_path(&base_link, "Failed to remove stale symlink")?;
        }

        Self::create_symlink(exec_path, &symlink)?;
        Ok(())
    }

    /// Removes a symbolic link by its package name
    pub fn remove_link(&self, name: &str) -> Result<()> {
        let base_link = self.symlinks_dir.join(name);
        let symlink = Self::platform_link_path(&base_link);

        Self::remove_link_path(&symlink, "Failed to remove symlink")?;
        if base_link != symlink {
            Self::remove_link_path(&base_link, "Failed to remove stale symlink")?;
        }

        Ok(())
    }

    pub fn rename_link(&self, old_name: &str, new_name: &str) -> Result<bool> {
        let old_base = self.symlinks_dir.join(old_name);
        let new_base = self.symlinks_dir.join(new_name);
        let old_link = Self::platform_link_path(&old_base);
        let new_link = Self::platform_link_path(&new_base);

        if path_exists_no_follow(&new_link)?
            || (new_base != new_link && path_exists_no_follow(&new_base)?)
        {
            return Err(anyhow!(
                "Refusing to overwrite existing runtime link for '{}'",
                new_name
            ));
        }

        let source = if path_exists_no_follow(&old_link)? {
            old_link
        } else if old_base != old_link && path_exists_no_follow(&old_base)? {
            old_base
        } else {
            return Ok(false);
        };

        fs::rename(&source, &new_link).with_context(|| {
            format!(
                "Failed to rename runtime link '{}' to '{}'",
                source.display(),
                new_link.display()
            )
        })?;
        Ok(true)
    }

    #[cfg(unix)]
    fn create_symlink(target_path: &Path, symlink: &Path) -> Result<()> {
        std::os::unix::fs::symlink(target_path, symlink).context("Failed to create symlink")
    }

    #[cfg(windows)]
    fn create_symlink(target_path: &Path, link: &Path) -> Result<()> {
        fs::hard_link(target_path, link).context("Failed to create hardlink")
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::SymlinkManager;
    #[cfg(unix)]
    use std::path::{Path, PathBuf};
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(unix)]
    use std::{fs, io};

    #[cfg(unix)]
    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-symlink-test-{name}-{nanos}"))
    }

    #[cfg(unix)]
    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[cfg(unix)]
    #[test]
    fn add_link_replaces_dangling_symlink() {
        let root = temp_root("replace-dangling");
        let symlinks_dir = root.join("symlinks");
        let missing_target = root.join("missing-target");
        let new_target = root.join("new-target");
        let link_name = "arduino";
        let link_path = symlinks_dir.join(link_name);

        fs::create_dir_all(&symlinks_dir).expect("create symlink dir");
        fs::write(&new_target, b"new-target").expect("write new target");
        std::os::unix::fs::symlink(&missing_target, &link_path).expect("create dangling symlink");
        assert!(
            !link_path.exists(),
            "dangling symlink should not exist via exists()"
        );
        assert!(
            fs::symlink_metadata(&link_path).is_ok(),
            "dangling symlink should still be present on disk"
        );

        let manager = SymlinkManager::new(&symlinks_dir);
        manager
            .add_link(&new_target, link_name)
            .expect("replace dangling symlink");

        let target = fs::read_link(&link_path).expect("read link target");
        assert_eq!(target, new_target);

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn remove_link_removes_dangling_symlink() {
        let root = temp_root("remove-dangling");
        let symlinks_dir = root.join("symlinks");
        let missing_target = root.join("missing-target");
        let link_name = "arduino";
        let link_path = symlinks_dir.join(link_name);

        fs::create_dir_all(&symlinks_dir).expect("create symlink dir");
        std::os::unix::fs::symlink(&missing_target, &link_path).expect("create dangling symlink");
        assert!(
            fs::symlink_metadata(&link_path).is_ok(),
            "dangling symlink should be present before removal"
        );

        let manager = SymlinkManager::new(&symlinks_dir);
        manager
            .remove_link(link_name)
            .expect("remove dangling symlink");

        assert!(
            fs::symlink_metadata(&link_path).is_err(),
            "dangling symlink should be removed"
        );

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn rename_link_moves_alias_without_overwriting_existing_target() {
        let root = temp_root("rename");
        let symlinks_dir = root.join("symlinks");
        let target = root.join("target");
        fs::create_dir_all(&symlinks_dir).expect("create symlink dir");
        fs::write(&target, b"target").expect("write target");
        let manager = SymlinkManager::new(&symlinks_dir);
        manager.add_link(&target, "old").expect("add old link");

        assert!(manager.rename_link("old", "new").expect("rename link"));
        assert!(fs::symlink_metadata(symlinks_dir.join("old")).is_err());
        assert_eq!(
            fs::read_link(symlinks_dir.join("new")).expect("read new link"),
            target
        );

        manager.add_link(&target, "old").expect("recreate old link");
        let error = manager
            .rename_link("old", "new")
            .expect_err("existing link should be preserved");
        assert!(error.to_string().contains("Refusing to overwrite"));
        assert!(fs::symlink_metadata(symlinks_dir.join("old")).is_ok());

        cleanup(&root).expect("cleanup");
    }
}
