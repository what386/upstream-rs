use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

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
}

impl BuildProfileHandler for RustProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Rust
    }

    fn detect(&self, workspace: &Path) -> bool {
        workspace.join("Cargo.toml").is_file()
    }

    fn run_build(
        &self,
        workspace: &Path,
        package_name: &str,
        output_override: Option<&Path>,
    ) -> Result<PathBuf> {
        let status = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(workspace)
            .status()
            .context("Failed to run 'cargo build --release'. Is Cargo installed?")?;

        if !status.success() {
            bail!("Cargo build failed for '{}'", package_name);
        }

        let candidate = if let Some(path) = output_override {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                workspace.join(path)
            }
        } else {
            workspace
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
