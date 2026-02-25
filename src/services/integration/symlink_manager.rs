use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

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
#[path = "../../../tests/services/integration/symlink_manager.rs"]
mod tests;
