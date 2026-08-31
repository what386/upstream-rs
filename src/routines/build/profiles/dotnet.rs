use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::routines::build::{
    BuildProfile,
    profiles::{BuildProfileHandler, emit_line_callback, run_command_with_line_callback},
};

pub struct DotnetProfile;

impl DotnetProfile {
    fn find_project_file(workspace: &Path) -> Option<PathBuf> {
        let mut projects = std::fs::read_dir(workspace)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "csproj"))
            .collect::<Vec<_>>();
        projects.sort();
        if let [project] = projects.as_slice() {
            return Some(project.clone());
        }

        let mut solutions = std::fs::read_dir(workspace)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext == "sln" || ext == "slnx")
            })
            .collect::<Vec<_>>();
        solutions.sort();
        let solution = solutions.first()?;
        let contents = std::fs::read_to_string(solution).ok()?;
        let projects = if solution.extension().is_some_and(|ext| ext == "slnx") {
            contents
                .lines()
                .filter_map(|line| line.split("Path=\"").nth(1))
                .filter_map(|path| path.split('"').next())
                .filter(|path| path.ends_with(".csproj"))
                .map(|path| workspace.join(path.replace('\\', "/")))
                .collect::<Vec<_>>()
        } else {
            contents
                .lines()
                .filter_map(|line| line.split(", \"").nth(1))
                .filter_map(|path| path.split('"').next())
                .filter(|path| path.ends_with(".csproj"))
                .map(|path| workspace.join(path.replace('\\', "/")))
                .collect::<Vec<_>>()
        };
        (projects.len() == 1).then(|| projects[0].clone())
    }

    fn project_properties(project: &Path) -> Result<(String, String)> {
        let output = Command::new("dotnet")
            .arg("msbuild")
            .arg(project)
            .arg("-nologo")
            .arg("-getProperty:OutputType")
            .arg("-getProperty:AssemblyName")
            .output()
            .context("Failed to inspect .NET project metadata. Is .NET SDK installed?")?;
        if !output.status.success() {
            bail!(
                ".NET project metadata inspection failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let properties: serde_json::Value = serde_json::from_slice(&output.stdout)
            .context(".NET returned invalid project metadata")?;
        let values = properties
            .get("Properties")
            .ok_or_else(|| anyhow!(".NET project metadata omitted Properties"))?;
        let output_type = values
            .get("OutputType")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !matches!(output_type.as_str(), "Exe" | "WinExe") {
            bail!(
                ".NET project output type '{}' is not executable",
                output_type
            );
        }
        let assembly_name = values
            .get("AssemblyName")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| anyhow!(".NET project metadata omitted AssemblyName"))?
            .to_string();
        Ok((output_type, assembly_name))
    }
}

impl BuildProfileHandler for DotnetProfile {
    fn profile(&self) -> BuildProfile {
        BuildProfile::Dotnet
    }

    fn detect(&self, workspace: &Path) -> bool {
        Self::find_project_file(workspace).is_some()
    }

    fn run_build(
        &self,
        workspace: &Path,
        package_name: &str,
        line_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<PathBuf> {
        let project = Self::find_project_file(workspace).ok_or_else(|| {
            anyhow!(
                "Could not find a .sln or .csproj in repository root '{}'.",
                workspace.display()
            )
        })?;

        let project_dir = project.parent().unwrap_or(workspace);
        let (_, assembly_name) = Self::project_properties(&project)?;
        let publish_dir = project_dir.join(".upstream-build").join("publish");
        std::fs::create_dir_all(&publish_dir).context(format!(
            "Failed to create dotnet publish directory '{}'",
            publish_dir.display()
        ))?;

        emit_line_callback(line_callback, "Running dotnet publish ...");
        let status = run_command_with_line_callback(
            Command::new("dotnet")
                .arg("publish")
                .arg(&project)
                .arg("-c")
                .arg("Release")
                .arg("-o")
                .arg(&publish_dir)
                .arg("-p:UseAppHost=true")
                .current_dir(&project_dir),
            "Failed to run 'dotnet publish'. Is .NET SDK installed?",
            line_callback,
        )?;

        if !status.success() {
            bail!("Dotnet publish failed for '{}'", package_name);
        }

        #[cfg(windows)]
        let artifact_name = format!("{assembly_name}.exe");
        #[cfg(not(windows))]
        let artifact_name = assembly_name;
        let artifact = publish_dir.join(artifact_name);
        if !artifact.is_file() {
            return Err(anyhow!(
                ".NET publish succeeded but expected AssemblyName artifact was not found at '{}'",
                artifact.display()
            ));
        }
        Ok(artifact)
    }
}
