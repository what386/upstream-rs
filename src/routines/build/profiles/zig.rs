use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::routines::build::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

pub struct ZigProfile;

impl ZigProfile {
    fn find_project_dir(workspace: &Path) -> Option<PathBuf> {
        if workspace.join("build.zig").is_file() {
            Some(workspace.to_path_buf())
        } else {
            None
        }
    }

    fn installed_executable_name(project_dir: &Path, preferred_name: &str) -> Result<String> {
        let source = std::fs::read_to_string(project_dir.join("build.zig"))
            .context("Failed to read build.zig")?;

        let mut declarations = Vec::new();
        let mut offset = 0;
        while let Some(relative) = source[offset..].find("addExecutable") {
            let start = offset + relative;
            let Some(object_start) = source[start..].find(".{") else {
                break;
            };

            let object_start = start + object_start;
            let Some(object_end) = source[object_start..].find("});") else {
                break;
            };

            let object = &source[object_start..object_start + object_end];
            let Some(name_start) = object.find(".name") else {
                offset = object_start + 2;
                continue;
            };

            let Some(quote_start) = object[name_start..].find('"') else {
                offset = object_start + 2;
                continue;
            };

            let value = &object[name_start + quote_start + 1..];
            let Some(quote_end) = value.find('"') else {
                offset = object_start + 2;
                continue;
            };

            let statement = &source[..start];
            let Some(const_start) = statement.rfind("const ") else {
                offset = object_start + 2;
                continue;
            };

            let binding = statement[const_start + "const ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches('=');

            if !binding.is_empty() {
                declarations.push((binding.to_string(), value[..quote_end].to_string()));
            }

            offset = object_start + 2;
        }

        let mut installed = declarations
            .iter()
            .filter(|(binding, _)| source.contains(&format!("installArtifact({binding})")))
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();

        installed.sort();
        installed.dedup();
        match installed.as_slice() {
            [] => bail!("build.zig has no statically named installed executable"),
            [name] => Ok(name.clone()),
            names => names
                .iter()
                .find(|name| name.as_str() == preferred_name)
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "build.zig has multiple installed executables ({}); none matches '{}'",
                        names.join(", "),
                        preferred_name
                    )
                }),
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
                .env(
                    "ZIG_LOCAL_CACHE_DIR",
                    project_dir.join(".upstream-build").join("zig-cache"),
                )
                .env(
                    "ZIG_GLOBAL_CACHE_DIR",
                    project_dir.join(".upstream-build").join("zig-global-cache"),
                )
                .current_dir(&project_dir),
            "Failed to run 'zig build -Doptimize=ReleaseSafe'. Is Zig installed?",
            line_callback,
        )?;

        if !status.success() {
            bail!("Zig build failed for '{}'", package_name);
        }

        let artifact_name = Self::installed_executable_name(&project_dir, package_name)?;
        let artifact = project_dir.join("zig-out").join("bin").join(&artifact_name);
        if !artifact.is_file() {
            return Err(anyhow!(
                "Zig build succeeded but declared installed executable '{}' was not found at '{}'",
                artifact_name,
                artifact.display()
            ));
        }

        Ok(artifact)
    }
}

#[cfg(test)]
mod tests {
    use super::ZigProfile;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();

        let root = std::env::temp_dir().join(format!("upstream-zig-profile-{nonce}"));
        fs::create_dir_all(&root).expect("create root");
        root
    }

    #[test]
    fn reads_declared_installed_executable_name() {
        let root = temp_root();
        fs::write(
            root.join("build.zig"),
            r#"const exe = b.addExecutable(.{ .name = "actual-bin" });
b.installArtifact(exe);"#,
        )
        .expect("write build file");

        assert_eq!(
            ZigProfile::installed_executable_name(&root, "friendly-name").expect("name"),
            "actual-bin"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rejects_dynamic_installed_executable_names() {
        let root = temp_root();
        fs::write(
            root.join("build.zig"),
            "const exe = b.addExecutable(.{ .name = project_name });\nb.installArtifact(exe);",
        )
        .expect("write build file");

        assert!(ZigProfile::installed_executable_name(&root, "friendly-name").is_err());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
