use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

pub struct ShellIntegrator<'a> {
    paths_file: &'a Path,
    symlinks_dir: &'a Path,
}

impl<'a> ShellIntegrator<'a> {
    pub fn new(paths_file: &'a Path, symlinks_dir: &'a Path) -> Self {
        Self {
            paths_file,
            symlinks_dir,
        }
    }

    /// Creates or updates the `path.sh` file with the symlinks directory
    pub fn initialize(&self) -> Result<()> {
        const HEADER: &str = "#!/bin/bash\n# Upstream managed PATH additions\n";

        let path_line = format!(
            "export PATH=\"{}:$PATH\"",
            self.symlinks_dir.to_string_lossy()
        );

        if !self.paths_file.exists() {
            let content = format!("{HEADER}{path_line}\n");
            fs::write(&self.paths_file, content)
                .context("Failed to create paths file")?;
            return Ok(());
        }

        // Ensure the line exists exactly once
        let existing = fs::read_to_string(&self.paths_file)
            .context("Failed to read paths file")?;

        if !existing.contains(&path_line) {
            let updated = format!("{existing}{path_line}\n");
            fs::write(&self.paths_file, updated)
                .context("Failed to update paths file")?;
        }

        Ok(())
    }

    /// Adds a package's installation path to PATH by appending an export line
    pub fn add_to_paths(&self, install_path: &Path) -> Result<()> {
        if !install_path.is_dir() {
            anyhow::bail!(
                "Package install directory not found: {}",
                install_path.to_string_lossy()
            );
        }

        let mut content = fs::read_to_string(&self.paths_file)
            .context("Failed to read paths file")?;

        let escaped = install_path.to_string_lossy()
            .replace('$', "\\$")
            .replace('"', "\\\"");

        let export_line = format!("export PATH=\"{escaped}:$PATH\"");

        if !content.contains(&export_line) {
            content.push_str(&format!("{export_line}\n"));
            fs::write(&self.paths_file, &content)
                .context("Failed to write paths file")?;
        }

        Ok(())
    }

    /// Removes a package's PATH entry from `path.sh`
    pub fn remove_from_paths(&self, install_path: &Path) -> Result<()> {
        let mut content = fs::read_to_string(&self.paths_file)
            .context("Failed to read paths file")?;

        let escaped = install_path.to_string_lossy()
            .replace('$', "\\$")
            .replace('"', "\\\"");

        let export_line = format!("export PATH=\"{escaped}:$PATH\"");

        content = content.replace(&format!("{export_line}\n"), "");
        content = content.replace(&export_line, "");

        fs::write(&self.paths_file, content)
            .context("Failed to write paths file")?;

        Ok(())
    }
}
