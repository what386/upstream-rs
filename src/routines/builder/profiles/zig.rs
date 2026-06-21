use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow, bail};

use crate::routines::builder::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

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
        line_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf> {
        let project_dir = Self::find_project_dir(workspace).ok_or_else(|| {
            anyhow!(
                "Could not find build.zig in repository root '{}'.",
                workspace.display()
            )
        })?;

        emit_line_callback(
            line_callback,
            "Running zig build -Doptimize=ReleaseSafe ...",
        );
        let status = run_command_with_line_callback(
            Command::new("zig")
                .arg("build")
                .arg("-Doptimize=ReleaseSafe")
                .current_dir(&project_dir),
            "Failed to run 'zig build -Doptimize=ReleaseSafe'. Is Zig installed?",
            line_callback,
        )?;

        if !status.success() {
            bail!("Zig build failed for '{}'", package_name);
        }

        let artifact = project_dir
            .join("zig-out")
            .join("bin")
            .join(Self::binary_name(package_name));

        if !artifact.exists() {
            return Err(anyhow!(
                "Zig build succeeded but artifact was not found at '{}'",
                artifact.display()
            ));
        }

        Ok(artifact)
    }
}
