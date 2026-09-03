use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::routines::build::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

pub struct CmakeProfile;

impl CmakeProfile {
    fn find_project_dir(workspace: &Path) -> Option<PathBuf> {
        if workspace.join("CMakeLists.txt").is_file() {
            Some(workspace.to_path_buf())
        } else {
            None
        }
    }

    fn request_codemodel(build_dir: &Path) -> Result<()> {
        let query_dir = build_dir
            .join(".cmake")
            .join("api")
            .join("v1")
            .join("query");

        std::fs::create_dir_all(&query_dir).context(format!(
            "Failed to create CMake File API query directory '{}'",
            query_dir.display()
        ))?;
        std::fs::write(query_dir.join("codemodel-v2"), [])
            .context("Failed to request CMake codemodel")
    }

    fn find_artifact(build_dir: &Path, preferred_name: &str) -> Result<(String, PathBuf)> {
        let reply_dir = build_dir
            .join(".cmake")
            .join("api")
            .join("v1")
            .join("reply");

        let mut indexes = std::fs::read_dir(&reply_dir)
            .context(format!(
                "Failed to read CMake File API reply directory '{}'",
                reply_dir.display()
            ))?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name().is_some_and(|name| {
                    name.to_string_lossy().starts_with("index-")
                        && path
                            .extension()
                            .is_some_and(|extension| extension == "json")
                })
            })
            .collect::<Vec<_>>();

        indexes.sort();
        let index = indexes
            .last()
            .ok_or_else(|| anyhow!("CMake did not produce a File API reply"))?;

        let index: serde_json::Value = serde_json::from_slice(&std::fs::read(index)?)
            .context("CMake returned invalid File API index")?;

        let codemodel_file = index
            .get("reply")
            .and_then(|reply| {
                reply
                    .as_array()
                    .into_iter()
                    .flatten()
                    .chain(
                        reply
                            .as_object()
                            .into_iter()
                            .flat_map(|replies| replies.values()),
                    )
                    .find(|reply| {
                        reply
                            .get("kind")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|kind| {
                                kind == "codemodel" || kind.starts_with("codemodel-")
                            })
                    })
                    .and_then(|reply| reply.get("jsonFile").and_then(serde_json::Value::as_str))
            })
            .ok_or_else(|| anyhow!("CMake File API reply omitted the codemodel"))?;

        let codemodel: serde_json::Value =
            serde_json::from_slice(&std::fs::read(reply_dir.join(codemodel_file))?)
                .context("CMake returned invalid codemodel")?;

        let configuration = codemodel
            .get("configurations")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .find(|configuration| {
                configuration
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    == Some("Release")
            })
            .or_else(|| {
                codemodel
                    .get("configurations")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|configurations| configurations.first())
            })
            .ok_or_else(|| anyhow!("CMake codemodel contains no configurations"))?;

        let mut targets = Vec::new();
        for target in configuration
            .get("targets")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(json_file) = target.get("jsonFile").and_then(serde_json::Value::as_str) else {
                continue;
            };

            let target: serde_json::Value =
                serde_json::from_slice(&std::fs::read(reply_dir.join(json_file))?)
                    .context("CMake returned invalid target metadata")?;

            if target.get("type").and_then(serde_json::Value::as_str) != Some("EXECUTABLE") {
                continue;
            }

            let Some(name) = target.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };

            let Some(path) = target
                .get("artifacts")
                .and_then(serde_json::Value::as_array)
                .and_then(|artifacts| artifacts.first())
                .and_then(|artifact| artifact.get("path"))
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };

            let artifact = PathBuf::from(path);
            let artifact = if artifact.is_absolute() {
                artifact
            } else {
                build_dir.join(artifact)
            };

            targets.push((name.to_string(), artifact));
        }

        targets.sort_by(|left, right| left.0.cmp(&right.0));
        match targets.as_slice() {
            [] => bail!("CMake codemodel contains no executable targets"),
            [target] => Ok(target.clone()),
            targets => targets
                .iter()
                .find(|target| target.0 == preferred_name)
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "CMake has multiple executable targets ({}); none matches '{}'",
                        targets
                            .iter()
                            .map(|target| target.0.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        preferred_name
                    )
                }),
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

        Self::request_codemodel(&build_dir)?;

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
        let (target_name, artifact) = Self::find_artifact(&build_dir, package_name)?;
        let build = run_command_with_line_callback(
            Command::new("cmake")
                .arg("--build")
                .arg(&build_dir)
                .arg("--config")
                .arg("Release")
                .arg("--target")
                .arg(&target_name)
                .current_dir(&project_dir),
            "Failed to run 'cmake --build <build-dir> --config Release'. Is CMake installed?",
            line_callback,
        )?;

        if !build.success() {
            bail!("CMake build failed for '{}'", package_name);
        }

        Ok(artifact)
    }
}

#[cfg(test)]
mod tests {
    use super::CmakeProfile;
    use std::fs;
    use std::path::{Path, PathBuf};
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

    fn write_codemodel(root: &Path, targets: &[(&str, &str)]) {
        let reply = root.join(".cmake/api/v1/reply");
        fs::create_dir_all(&reply).expect("create reply directory");
        let mut target_entries = Vec::new();
        for (index, (name, artifact)) in targets.iter().enumerate() {
            let file = format!("target-{index}.json");
            fs::write(
                reply.join(&file),
                format!(r#"{{"type":"EXECUTABLE","name":"{name}","artifacts":[{{"path":"{artifact}"}}]}}"#),
            )
            .expect("write target metadata");
            target_entries.push(format!(r#"{{"name":"{name}","jsonFile":"{file}"}}"#));
        }

        fs::write(
            reply.join("codemodel.json"),
            format!(
                r#"{{"configurations":[{{"name":"Release","targets":[{}]}}]}}"#,
                target_entries.join(",")
            ),
        )
        .expect("write codemodel");
        fs::write(
            reply.join("index-test.json"),
            r#"{"reply":[{"kind":"codemodel","jsonFile":"codemodel.json"}]}"#,
        )
        .expect("write index");
    }

    #[test]
    fn find_artifact_uses_cmake_target_metadata() {
        let root = temp_root("metadata");
        let artifact = root.join("bin").join("actual-name");
        write_codemodel(
            &root,
            &[("actual-name", artifact.to_str().expect("artifact path"))],
        );

        let found = CmakeProfile::find_artifact(&root, "friendly-name").expect("find artifact");

        assert_eq!(found, ("actual-name".to_string(), artifact));
    }
}
