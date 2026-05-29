use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::services::builder::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

pub struct GoProfile;

impl GoProfile {
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
        if workspace.join("go.mod").is_file() {
            Some(workspace.to_path_buf())
        } else {
            None
        }
    }
}

impl BuildProfileHandler for GoProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Go
    }

    fn detect(&self, workspace: &Path) -> bool {
        Self::find_project_dir(workspace).is_some()
    }

    fn run_build(
        &self,
        workspace: &Path,
        package_name: &str,
        output_override: Option<&Path>,
        line_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf> {
        let project_dir = Self::find_project_dir(workspace).ok_or_else(|| {
            anyhow!(
                "Could not find go.mod in repository root '{}'.",
                workspace.display()
            )
        })?;

        let artifact = if let Some(path) = output_override {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            }
        } else {
            project_dir
                .join(".upstream-build")
                .join(Self::binary_name(package_name))
        };

        if let Some(parent) = artifact.parent() {
            std::fs::create_dir_all(parent).context(format!(
                "Failed to create go output directory '{}'",
                parent.display()
            ))?;
        }

        emit_line_callback(line_callback, "Running go build -o <artifact> . ...");
        let root_status = run_command_with_line_callback(
            Command::new("go")
                .arg("build")
                .arg("-o")
                .arg(&artifact)
                .arg(".")
                .current_dir(&project_dir),
            "Failed to run 'go build -o <artifact> .'. Is Go installed?",
            line_callback,
        )?;

        if !root_status.success() {
            let cmd_target = format!("./cmd/{package_name}");
            let context = format!(
                "Failed to run fallback 'go build -o <artifact> {cmd_target}'. Is Go installed?"
            );
            emit_line_callback(
                line_callback,
                format!("Running go build -o <artifact> {cmd_target} ..."),
            );
            let cmd_status = run_command_with_line_callback(
                Command::new("go")
                    .arg("build")
                    .arg("-o")
                    .arg(&artifact)
                    .arg(&cmd_target)
                    .current_dir(&project_dir),
                &context,
                line_callback,
            )?;

            if !cmd_status.success() {
                bail!(
                    "Go build failed for '{}': both '.' and './cmd/{}' targets failed",
                    package_name,
                    package_name
                );
            }
        }

        if !artifact.exists() {
            return Err(anyhow!(
                "Go build succeeded but artifact was not found at '{}'. \
                 Use --build-output to provide a custom path.",
                artifact.display()
            ));
        }

        Ok(artifact)
    }
}
