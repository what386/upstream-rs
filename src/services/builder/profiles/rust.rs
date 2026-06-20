use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, str::FromStr};

use anyhow::{Result, anyhow, bail};

use crate::services::builder::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

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
            Some(workspace.to_path_buf())
        } else {
            None
        }
    }

    fn has_multiple_declared_bins(project_dir: &Path) -> bool {
        let cargo_toml_path = project_dir.join("Cargo.toml");
        let Ok(cargo_toml) = fs::read_to_string(&cargo_toml_path) else {
            return false;
        };

        let Ok(parsed) = toml::Value::from_str(&cargo_toml) else {
            return false;
        };

        parsed
            .get("bin")
            .and_then(toml::Value::as_array)
            .is_some_and(|bins| bins.len() > 1)
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
        line_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf> {
        let project_dir = Self::find_project_dir(workspace).ok_or_else(|| {
            anyhow!(
                "Could not find Cargo.toml in repository root '{}'.",
                workspace.display()
            )
        })?;

        let status = if Self::has_multiple_declared_bins(&project_dir) {
            emit_line_callback(line_callback, "Running cargo build --release --bin ...");
            run_command_with_line_callback(
                Command::new("cargo")
                    .arg("build")
                    .arg("--release")
                    .arg("--bin")
                    .arg(package_name)
                    .current_dir(&project_dir),
                "Failed to run 'cargo build --release --bin <name>'. Is Cargo installed?",
                line_callback,
            )?
        } else {
            emit_line_callback(line_callback, "Running cargo build --release ...");
            run_command_with_line_callback(
                Command::new("cargo")
                    .arg("build")
                    .arg("--release")
                    .current_dir(&project_dir),
                "Failed to run 'cargo build --release'. Is Cargo installed?",
                line_callback,
            )?
        };

        if !status.success() {
            bail!("Cargo build failed for '{}'", package_name);
        }

        let candidate = project_dir
            .join("target")
            .join("release")
            .join(Self::binary_name(package_name));

        if !candidate.exists() {
            return Err(anyhow!(
                "Rust build succeeded but artifact was not found at '{}'",
                candidate.display()
            ));
        }

        Ok(candidate)
    }
}
