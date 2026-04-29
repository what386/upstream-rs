use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};

use crate::services::builder::{BuildProfile, profiles::BuildProfileHandler};

pub struct CmakeProfile;

impl CmakeProfile {
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
        if workspace.join("CMakeLists.txt").is_file() {
            Some(workspace.to_path_buf())
        } else {
            None
        }
    }
}

impl BuildProfileHandler for CmakeProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Cmake
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
                "Could not find CMakeLists.txt in repository root '{}'.",
                workspace.display()
            )
        })?;

        let build_dir = project_dir.join(".upstream-build").join("cmake");
        std::fs::create_dir_all(&build_dir).context(format!(
            "Failed to create CMake build directory '{}'",
            build_dir.display()
        ))?;

        let configure = Command::new("cmake")
            .arg("-S")
            .arg(&project_dir)
            .arg("-B")
            .arg(&build_dir)
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .current_dir(&project_dir)
            .stdin(Stdio::null())
            .status()
            .context("Failed to run 'cmake -S . -B <build-dir> -DCMAKE_BUILD_TYPE=Release'. Is CMake installed?")?;

        if !configure.success() {
            bail!("CMake configure failed for '{}'", package_name);
        }

        let build = Command::new("cmake")
            .arg("--build")
            .arg(&build_dir)
            .arg("--config")
            .arg("Release")
            .current_dir(&project_dir)
            .stdin(Stdio::null())
            .status()
            .context("Failed to run 'cmake --build <build-dir> --config Release'. Is CMake installed?")?;

        if !build.success() {
            bail!("CMake build failed for '{}'", package_name);
        }

        let artifact = if let Some(path) = output_override {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            }
        } else {
            build_dir.join(Self::binary_name(package_name))
        };

        if !artifact.exists() {
            return Err(anyhow!(
                "CMake build succeeded but artifact was not found at '{}'. \
                 Use --build-output to provide a custom path.",
                artifact.display()
            ));
        }

        Ok(artifact)
    }
}
