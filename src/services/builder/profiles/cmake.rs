use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::services::builder::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

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
        line_callback: &mut Option<&mut dyn FnMut(&str)>,
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

        emit_line_callback(line_callback, "Running cmake configure ...");
        let configure = run_command_with_line_callback(
            Command::new("cmake")
                .arg("-S")
                .arg(&project_dir)
                .arg("-B")
                .arg(&build_dir)
                .arg("-DCMAKE_BUILD_TYPE=Release")
                .current_dir(&project_dir),
            "Failed to run 'cmake -S . -B <build-dir> -DCMAKE_BUILD_TYPE=Release'. Is CMake installed?",
            line_callback,
        )?;

        if !configure.success() {
            bail!("CMake configure failed for '{}'", package_name);
        }

        emit_line_callback(line_callback, "Running cmake build ...");
        let build = run_command_with_line_callback(
            Command::new("cmake")
                .arg("--build")
                .arg(&build_dir)
                .arg("--config")
                .arg("Release")
                .current_dir(&project_dir),
            "Failed to run 'cmake --build <build-dir> --config Release'. Is CMake installed?",
            line_callback,
        )?;

        if !build.success() {
            bail!("CMake build failed for '{}'", package_name);
        }

        let artifact = build_dir.join(Self::binary_name(package_name));

        if !artifact.exists() {
            return Err(anyhow!(
                "CMake build succeeded but artifact was not found at '{}'",
                artifact.display()
            ));
        }

        Ok(artifact)
    }
}
