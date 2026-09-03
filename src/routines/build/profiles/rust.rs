use std::path::{Path, PathBuf};
use std::{fs, process::Command};

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

    fn target_directory(project_dir: &Path) -> PathBuf {
        if let Some(target_dir) = std::env::var_os("CARGO_TARGET_DIR") {
            return PathBuf::from(target_dir);
        }

        let mut configured = None;
        let mut ancestors = project_dir.ancestors().collect::<Vec<_>>();
        ancestors.reverse();
        for ancestor in ancestors {
            for file_name in ["config.toml", "config"] {
                let path = ancestor.join(".cargo").join(file_name);
                let Ok(contents) = fs::read_to_string(&path) else {
                    continue;
                };

                let Ok(config): std::result::Result<toml::Value, _> = toml::from_str(&contents)
                else {
                    continue;
                };

                if let Some(target_dir) = config
                    .get("build")
                    .and_then(|build| build.get("target-dir"))
                    .and_then(toml::Value::as_str)
                {
                    let target_dir = PathBuf::from(target_dir);
                    configured = Some(if target_dir.is_absolute() {
                        target_dir
                    } else {
                        path.parent().unwrap().parent().unwrap().join(target_dir)
                    });
                }
            }
        }

        configured.unwrap_or_else(|| project_dir.join("target"))
    }

    fn cargo_binary_target(project_dir: &Path, preferred_name: &str) -> Result<(String, PathBuf)> {
        let manifest_path = project_dir.join("Cargo.toml");
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(&manifest_path).with_context(|| {
                format!(
                    "Failed to read Cargo manifest '{}'.",
                    manifest_path.display()
                )
            })?)
            .with_context(|| format!("Failed to parse '{}'.", manifest_path.display()))?;

        let mut package_dirs = vec![project_dir.to_path_buf()];
        if let Some(members) = manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("members"))
            .and_then(toml::Value::as_array)
        {
            for member in members.iter().filter_map(toml::Value::as_str) {
                let pattern = project_dir.join(member);
                let pattern = pattern.to_string_lossy();
                if member.contains('*') || member.contains('?') || member.contains('[') {
                    package_dirs.extend(
                        glob::glob(&pattern)
                            .with_context(|| {
                                format!("Invalid workspace member pattern '{member}'")
                            })?
                            .flatten()
                            .filter(|path| path.join("Cargo.toml").is_file()),
                    );
                } else if pattern.as_ref().ends_with("Cargo.toml") {
                    package_dirs.push(
                        PathBuf::from(pattern.as_ref())
                            .parent()
                            .unwrap()
                            .to_path_buf(),
                    );
                } else {
                    package_dirs.push(PathBuf::from(pattern.as_ref()));
                }
            }
        }

        let mut targets = Vec::new();
        for package_dir in package_dirs {
            let path = package_dir.join("Cargo.toml");
            let Ok(package) = fs::read_to_string(&path) else {
                continue;
            };

            let package: toml::Value = toml::from_str(&package)
                .with_context(|| format!("Failed to parse '{}'.", path.display()))?;

            if let Some(bins) = package.get("bin").and_then(toml::Value::as_array) {
                targets.extend(bins.iter().filter_map(|bin| {
                    bin.get("name")
                        .and_then(toml::Value::as_str)
                        .map(str::to_string)
                }));
            } else if package_dir.join("src/main.rs").is_file()
                && let Some(name) = package
                    .get("package")
                    .and_then(|package| package.get("name"))
                    .and_then(toml::Value::as_str)
            {
                targets.push(name.to_string());
            }
        }

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

        let target_directory = Self::target_directory(project_dir);

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
