use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};

use crate::services::builder::{BuildProfile, profiles::BuildProfileHandler};

pub struct ZigProfile;

impl ZigProfile {
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
        if workspace.join("build.zig").is_file() {
            Some(workspace.to_path_buf())
        } else {
            None
        }
    }
}

impl BuildProfileHandler for ZigProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Zig
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
                "Could not find build.zig in repository root '{}'.",
                workspace.display()
            )
        })?;

        let status = Command::new("zig")
            .arg("build")
            .arg("-Doptimize=ReleaseSafe")
            .current_dir(&project_dir)
            .stdin(Stdio::null())
            .status()
            .context("Failed to run 'zig build -Doptimize=ReleaseSafe'. Is Zig installed?")?;

        if !status.success() {
            bail!("Zig build failed for '{}'", package_name);
        }

        let artifact = if let Some(path) = output_override {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            }
        } else {
            project_dir
                .join("zig-out")
                .join("bin")
                .join(Self::binary_name(package_name))
        };

        if !artifact.exists() {
            return Err(anyhow!(
                "Zig build succeeded but artifact was not found at '{}'. \
                 Use --build-output to provide a custom path.",
                artifact.display()
            ));
        }

        Ok(artifact)
    }
}
