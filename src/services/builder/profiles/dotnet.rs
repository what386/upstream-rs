use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use walkdir::WalkDir;

use crate::services::builder::{BuildProfile, profiles::BuildProfileHandler};

pub struct DotnetProfile;

impl DotnetProfile {
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
        let has_root = std::fs::read_dir(workspace).ok().is_some_and(|entries| {
            entries.flatten().any(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "sln" || ext == "csproj")
            })
        });
        if has_root {
            return Some(workspace.to_path_buf());
        }

        WalkDir::new(workspace)
            .max_depth(4)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                entry.file_type().is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext == "sln" || ext == "csproj")
            })
            .and_then(|entry| entry.path().parent().map(Path::to_path_buf))
    }
}

impl BuildProfileHandler for DotnetProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Dotnet
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
                "Could not find a .sln or .csproj in '{}' (searched recursively).",
                workspace.display()
            )
        })?;

        let publish_dir = project_dir.join(".upstream-build").join("publish");
        std::fs::create_dir_all(&publish_dir).context(format!(
            "Failed to create dotnet publish directory '{}'",
            publish_dir.display()
        ))?;

        let status = Command::new("dotnet")
            .arg("publish")
            .arg("-c")
            .arg("Release")
            .arg("-o")
            .arg(&publish_dir)
            .current_dir(&project_dir)
            .status()
            .context("Failed to run 'dotnet publish'. Is .NET SDK installed?")?;

        if !status.success() {
            bail!("Dotnet publish failed for '{}'", package_name);
        }

        let candidate = if let Some(path) = output_override {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            }
        } else {
            publish_dir.join(Self::binary_name(package_name))
        };

        if !candidate.exists() {
            return Err(anyhow!(
                "Dotnet publish succeeded but artifact was not found at '{}'. \
                 Use --build-output to provide a custom path.",
                candidate.display()
            ));
        }

        Ok(candidate)
    }
}
