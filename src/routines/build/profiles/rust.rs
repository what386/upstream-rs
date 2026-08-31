use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::routines::build::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

pub struct RustProfile;

impl RustProfile {
    fn binary_name(target_name: &str) -> String {
        #[cfg(windows)]
        {
            format!("{target_name}.exe")
        }

        #[cfg(not(windows))]
        {
            target_name.to_string()
        }
    }

    fn find_project_dir(workspace: &Path) -> Option<PathBuf> {
        if workspace.join("Cargo.toml").is_file() {
            Some(workspace.to_path_buf())
        } else {
            None
        }
    }

    fn cargo_binary_target(project_dir: &Path, preferred_name: &str) -> Result<(String, PathBuf)> {
        let metadata = Command::new("cargo")
            .arg("metadata")
            .arg("--no-deps")
            .arg("--format-version")
            .arg("1")
            .current_dir(project_dir)
            .output()
            .context("Failed to inspect Cargo metadata. Is Cargo installed?")?;
        if !metadata.status.success() {
            bail!(
                "Cargo metadata failed: {}",
                String::from_utf8_lossy(&metadata.stderr).trim()
            );
        }

        let metadata: serde_json::Value =
            serde_json::from_slice(&metadata.stdout).context("Cargo returned invalid metadata")?;
        let targets: Vec<String> = metadata
            .get("packages")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|package| {
                package
                    .get("targets")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|target| {
                        target
                            .get("kind")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
                    })
                    .filter_map(|target| {
                        target
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
            })
            .collect();

        let target = match targets.as_slice() {
            [] => bail!("Cargo workspace has no binary target."),
            [target] => target.clone(),
            targets => {
                let matches = targets
                    .iter()
                    .filter(|target| target.as_str() == preferred_name)
                    .collect::<Vec<_>>();
                if matches.len() == 1 {
                    matches[0].clone()
                } else {
                    bail!(
                        "Cargo workspace has multiple binary targets ({}); '{}' is not an unambiguous target name.",
                        targets.join(", "),
                        preferred_name
                    )
                }
            }
        };

        let target_directory = metadata
            .get("target_directory")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("Cargo metadata did not provide a target directory"))?;

        Ok((target, target_directory))
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

        let (target_name, target_directory) =
            Self::cargo_binary_target(&project_dir, package_name)?;
        let status = {
            emit_line_callback(line_callback, "Running cargo build --release --bin ...");
            run_command_with_line_callback(
                Command::new("cargo")
                    .arg("build")
                    .arg("--release")
                    .arg("--bin")
                    .arg(&target_name)
                    .current_dir(&project_dir),
                "Failed to run 'cargo build --release --bin <name>'. Is Cargo installed?",
                line_callback,
            )?
        };

        if !status.success() {
            bail!("Cargo build failed for '{}'", package_name);
        }

        let candidate = target_directory
            .join("release")
            .join(Self::binary_name(&target_name));

        if !candidate.exists() {
            return Err(anyhow!(
                "Rust build succeeded but artifact was not found at '{}'",
                candidate.display()
            ));
        }

        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::RustProfile;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn cargo_metadata_resolves_binary_from_virtual_workspace_member() {
        let root = std::env::temp_dir().join(format!(
            "upstream-rust-profile-workspace-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("riprip/src")).expect("create member");
        fs::create_dir_all(root.join("riprip_core/src")).expect("create core member");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"3\"\nmembers = [\"riprip\", \"riprip_core\"]\n",
        )
        .expect("write workspace manifest");
        fs::write(
            root.join("riprip/Cargo.toml"),
            "[package]\nname = \"riprip\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"actual-riprip\"\npath = \"src/main.rs\"\n",
        )
        .expect("write binary manifest");
        fs::write(root.join("riprip/src/main.rs"), "fn main() {}\n").expect("write main");
        fs::write(
            root.join("riprip_core/Cargo.toml"),
            "[package]\nname = \"riprip_core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write library manifest");
        fs::write(root.join("riprip_core/src/lib.rs"), "pub fn core() {}\n")
            .expect("write library");

        let (target, _) = RustProfile::cargo_binary_target(&root, "friendly-name")
            .expect("resolve workspace binary");
        assert_eq!(target, "actual-riprip");

        fs::remove_dir_all(root).expect("cleanup");
    }
}
