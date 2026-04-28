use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use walkdir::WalkDir;

use crate::services::builder::{BuildProfile, profiles::BuildProfileHandler};

pub struct RustProfile;

impl RustProfile {
    fn binary_name(package_name: &str) -> String {
        #[cfg(windows)]
        {
            format!("{package_name}.exe")
        }
        #[cfg(not(windows))]
        {
            package_name.to_string()
        }
    }

    fn find_project_dir(workspace: &Path) -> Option<PathBuf> {
        if workspace.join("Cargo.toml").is_file() {
            return Some(workspace.to_path_buf());
        }

        WalkDir::new(workspace)
            .max_depth(4)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.file_type().is_file() && entry.file_name() == "Cargo.toml")
            .and_then(|entry| entry.path().parent().map(Path::to_path_buf))
    }
}

impl BuildProfileHandler for RustProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Rust
    }

    fn detect(&self, workspace: &Path) -> bool {
        Self::find_project_dir(workspace).is_some()
    }

    fn run_build(
        &self,
        workspace: &Path,
        package_name: &str,
        output_override: Option<&Path>,
    ) -> Result<PathBuf> {
        let project_dir = Self::find_project_dir(workspace).ok_or_else(|| {
            anyhow!(
                "Could not find Cargo.toml in '{}' (searched recursively).",
                workspace.display()
            )
        })?;

        let status = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(&project_dir)
            .status()
            .context("Failed to run 'cargo build --release'. Is Cargo installed?")?;

        if !status.success() {
            bail!("Cargo build failed for '{}'", package_name);
        }

        let candidate = if let Some(path) = output_override {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            }
        } else {
            project_dir
                .join("target")
                .join("release")
                .join(Self::binary_name(package_name))
        };

        if !candidate.exists() {
            return Err(anyhow!(
                "Rust build succeeded but artifact was not found at '{}'. \
                 Use --build-output to provide a custom path.",
                candidate.display()
            ));
        }

        Ok(candidate)
    }
}
