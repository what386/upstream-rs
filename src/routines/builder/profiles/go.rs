use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::routines::builder::{
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

    fn command_target(project_dir: &Path, package_name: &str) -> Option<String> {
        let command_dir = project_dir.join("cmd").join(package_name);
        if command_dir.is_dir() {
            Some(format!("./cmd/{package_name}"))
        } else {
            None
        }
    }

    fn artifact_is_executable(artifact: &Path) -> Result<bool> {
        let metadata = std::fs::metadata(artifact).context(format!(
            "Failed to read Go artifact metadata '{}'",
            artifact.display()
        ))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            Ok(metadata.permissions().mode() & 0o111 != 0)
        }

        #[cfg(not(unix))]
        {
            Ok(metadata.is_file())
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
        line_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf> {
        let project_dir = Self::find_project_dir(workspace).ok_or_else(|| {
            anyhow!(
                "Could not find go.mod in repository root '{}'.",
                workspace.display()
            )
        })?;

        let artifact = project_dir
            .join(".upstream-build")
            .join(Self::binary_name(package_name));

        if let Some(parent) = artifact.parent() {
            std::fs::create_dir_all(parent).context(format!(
                "Failed to create go output directory '{}'",
                parent.display()
            ))?;
        }

        let target = Self::command_target(&project_dir, package_name).unwrap_or_else(|| ".".into());
        let context = format!("Failed to run 'go build -o <artifact> {target}'. Is Go installed?");
        emit_line_callback(
            line_callback,
            format!("Running go build -o <artifact> {target} ..."),
        );
        let status = run_command_with_line_callback(
            Command::new("go")
                .arg("build")
                .arg("-o")
                .arg(&artifact)
                .arg(&target)
                .current_dir(&project_dir),
            &context,
            line_callback,
        )?;

        if !status.success() {
            bail!(
                "Go build failed for '{}' using target '{}'",
                package_name,
                target
            );
        }

        if !artifact.exists() {
            return Err(anyhow!(
                "Go build succeeded but artifact was not found at '{}'",
                artifact.display()
            ));
        }

        if !Self::artifact_is_executable(&artifact)? {
            return Err(anyhow!(
                "Go build output '{}' is not executable; '{}' appears to be a library package, not a command. Use --build-profile go with a package name that matches ./cmd/<name> or build a main package.",
                artifact.display(),
                target
            ));
        }

        Ok(artifact)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::GoProfile;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-go-profile-test-{name}-{nanos}"))
    }

    #[test]
    fn command_target_prefers_matching_cmd_package() {
        let root = temp_root("cmd-target");
        fs::create_dir_all(root.join("cmd").join("zsync")).expect("create command dir");

        let target = GoProfile::command_target(&root, "zsync");

        assert_eq!(target.as_deref(), Some("./cmd/zsync"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn command_target_returns_none_without_matching_cmd_package() {
        let root = temp_root("no-cmd-target");
        fs::create_dir_all(&root).expect("create root");

        let target = GoProfile::command_target(&root, "zsync");

        assert!(target.is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
