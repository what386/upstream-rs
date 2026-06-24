use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::routines::builder::{
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

    fn find_artifact(build_dir: &Path, binary_name: &str) -> Result<PathBuf> {
        let ordered_candidates = [
            build_dir.join(binary_name),
            build_dir.join("src").join(binary_name),
            build_dir.join("bin").join(binary_name),
            build_dir.join("Release").join(binary_name),
            build_dir.join("src").join("Release").join(binary_name),
            build_dir.join("bin").join("Release").join(binary_name),
        ];

        for candidate in ordered_candidates {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        let mut recursive_candidates = Self::find_exact_named_files(build_dir, binary_name)?;
        recursive_candidates.sort();

        match recursive_candidates.len() {
            0 => Err(anyhow!(
                "CMake build succeeded but artifact '{}' was not found under '{}'",
                binary_name,
                build_dir.display()
            )),
            1 => Ok(recursive_candidates.remove(0)),
            _ => {
                let listed = recursive_candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(anyhow!(
                    "CMake build produced multiple artifact candidates named '{}' under '{}': {}",
                    binary_name,
                    build_dir.display(),
                    listed
                ))
            }
        }
    }

    fn find_exact_named_files(root: &Path, binary_name: &str) -> Result<Vec<PathBuf>> {
        let mut candidates = Vec::new();
        let mut queue = VecDeque::from([root.to_path_buf()]);

        while let Some(dir) = queue.pop_front() {
            let entries = std::fs::read_dir(&dir).context(format!(
                "Failed to inspect CMake build directory '{}'",
                dir.display()
            ))?;

            for entry in entries {
                let entry = entry.context(format!(
                    "Failed to inspect CMake build directory '{}'",
                    dir.display()
                ))?;
                let path = entry.path();
                let file_type = entry.file_type().context(format!(
                    "Failed to inspect CMake build path '{}'",
                    path.display()
                ))?;

                if file_type.is_dir() {
                    if is_ignored_artifact_search_dir(&entry.file_name()) {
                        continue;
                    }
                    queue.push_back(path);
                } else if file_type.is_file() && entry.file_name() == binary_name {
                    candidates.push(path);
                }
            }
        }

        Ok(candidates)
    }
}

fn is_ignored_artifact_search_dir(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some("CMakeFiles" | "_deps" | "Testing" | ".git")
    )
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

        let binary_name = Self::binary_name(package_name);
        let artifact = Self::find_artifact(&build_dir, &binary_name)?;

        Ok(artifact)
    }
}

#[cfg(test)]
mod tests {
    use super::CmakeProfile;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("upstream-cmake-profile-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn find_artifact_prefers_build_root() {
        let root = temp_root("root");
        let direct = root.join("tool");
        let nested = root.join("src").join("tool");
        fs::create_dir_all(nested.parent().expect("nested parent")).expect("create nested parent");
        fs::write(&direct, b"direct").expect("write direct");
        fs::write(&nested, b"nested").expect("write nested");

        let artifact = CmakeProfile::find_artifact(&root, "tool").expect("find artifact");

        assert_eq!(artifact, direct);
    }

    #[test]
    fn find_artifact_checks_src_build_subdir() {
        let root = temp_root("src");
        let artifact = root.join("src").join("task");
        fs::create_dir_all(artifact.parent().expect("artifact parent"))
            .expect("create artifact parent");
        fs::write(&artifact, b"task").expect("write artifact");

        let found = CmakeProfile::find_artifact(&root, "task").expect("find artifact");

        assert_eq!(found, artifact);
    }

    #[test]
    fn find_artifact_uses_unambiguous_recursive_exact_match() {
        let root = temp_root("recursive");
        let artifact = root.join("tools").join("cli").join("tool");
        fs::create_dir_all(artifact.parent().expect("artifact parent"))
            .expect("create artifact parent");
        fs::write(&artifact, b"tool").expect("write artifact");

        let found = CmakeProfile::find_artifact(&root, "tool").expect("find artifact");

        assert_eq!(found, artifact);
    }

    #[test]
    fn find_artifact_reports_ambiguous_recursive_matches() {
        let root = temp_root("ambiguous");
        let first = root.join("tools").join("tool");
        let second = root.join("apps").join("tool");
        fs::create_dir_all(first.parent().expect("first parent")).expect("create first parent");
        fs::create_dir_all(second.parent().expect("second parent")).expect("create second parent");
        fs::write(&first, b"first").expect("write first");
        fs::write(&second, b"second").expect("write second");

        let err = CmakeProfile::find_artifact(&root, "tool")
            .expect_err("ambiguous candidates should fail")
            .to_string();

        assert!(err.contains("multiple artifact candidates"));
        assert!(err.contains(&first.display().to_string()));
        assert!(err.contains(&second.display().to_string()));
    }
}
