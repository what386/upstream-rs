use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

pub struct SymlinkManager<'a> {
    symlinks_dir: &'a Path,
}

impl<'a> SymlinkManager<'a> {
    pub fn new(symlinks_dir: &'a Path) -> Self {
        Self {
            symlinks_dir
        }
    }

    /// Creates a symbolic link in the symlinks directory pointing to the target file
    pub fn add_link(&self, exec_path: &Path, name: &str) -> Result<()> {
        if !exec_path.exists() {
            anyhow::bail!("Target file not found: {}", exec_path.display());
        }

        let symlink = self.symlinks_dir.join(name);

        // Remove existing symlink if present
        if symlink.exists() {
            fs::remove_file(&symlink)
                .context("Failed to remove existing symlink")?;
        }

        Self::create_symlink(exec_path, &symlink)?;
        Ok(())
    }

    /// Removes a symbolic link by its package name
    pub fn remove_link(&self, name: &str) -> Result<()> {
        let symlink = self.symlinks_dir.join(name);

        if symlink.exists() {
            fs::remove_file(&symlink)
                .context("Failed to remove symlink")?;
        }

        Ok(())
    }

    /// Gets the path to a symlink if it exists
    pub fn get_symlink(&self, name: &str) -> Option<PathBuf> {
        let symlink = self.symlinks_dir.join(name);
        symlink.exists().then_some(symlink)
    }

    /// Lists all symlinks in the directory
    pub fn list_symlinks(&self) -> Result<Vec<String>> {
        let mut symlinks = Vec::new();

        if !self.symlinks_dir.exists() {
            return Ok(symlinks);
        }

        for entry in fs::read_dir(&self.symlinks_dir)
            .context("Failed to read symlinks directory")?
        {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                symlinks.push(name.to_string());
            }
        }

        Ok(symlinks)
    }

    #[cfg(unix)]
    fn create_symlink(target_path: &Path, symlink: &Path) -> Result<()> {
        std::os::unix::fs::symlink(target_path, symlink)
            .context("Failed to create symlink")
    }

    #[cfg(windows)]
    fn create_symlink(target_path: &Path, symlink: &Path) -> Result<()> {
        std::os::windows::fs::symlink_file(target_path, symlink)
            .context("Failed to create symlink")
    }
}
