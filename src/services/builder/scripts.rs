use std::path::{Path, PathBuf};
use std::process::Command;

use crate::application::output;
use anyhow::{Context, Result, anyhow, bail};

use super::profiles::run_command_with_line_callback;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildScriptAction {
    Install,
    Upgrade,
}

impl BuildScriptAction {
    fn primary_names(&self) -> &'static [&'static str] {
        match self {
            BuildScriptAction::Install => &["install.sh", "install.bash", "install.ps1"],
            BuildScriptAction::Upgrade => &["upgrade.sh", "upgrade.bash", "upgrade.ps1"],
        }
    }

    fn label(&self) -> &'static str {
        match self {
            BuildScriptAction::Install => "install",
            BuildScriptAction::Upgrade => "upgrade",
        }
    }
}

fn fallback_names(action: BuildScriptAction) -> &'static [&'static str] {
    match action {
        BuildScriptAction::Install => &[],
        BuildScriptAction::Upgrade => &["install.sh", "install.bash", "install.ps1"],
    }
}

fn resolve_script_from_names(workspace_root: &Path, names: &[&str]) -> Option<PathBuf> {
    for dir in [workspace_root.to_path_buf(), workspace_root.join("scripts")] {
        for name in names {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

pub fn script_for(action: BuildScriptAction, workspace_root: &Path) -> Option<PathBuf> {
    resolve_script_from_names(workspace_root, action.primary_names())
        .or_else(|| resolve_script_from_names(workspace_root, fallback_names(action)))
}

fn is_ps1(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
}

fn validate_script(path: &Path) -> Result<()> {
    if is_ps1(path) {
        #[cfg(windows)]
        return Ok(());

        #[cfg(not(windows))]
        bail!(
            "Build script '{}' is a PowerShell script, which is only supported on Windows",
            path.display()
        );
    }

    let content = std::fs::read(path)
        .with_context(|| format!("Failed to read build script '{}'", path.display()))?;
    if content.starts_with(b"#!") {
        return Ok(());
    }

    bail!(
        "Build script '{}' is missing a shebang. Add '#!' so the OS can resolve the interpreter.",
        path.display()
    );
}

fn command_preview(path: &Path) -> String {
    if is_ps1(path) {
        return format!(
            "powershell -ExecutionPolicy Bypass -File {}",
            path.display()
        );
    }

    path.display().to_string()
}

fn command_for(path: &Path) -> Result<Command> {
    if is_ps1(path) {
        #[cfg(windows)]
        {
            let mut command = Command::new("powershell");
            command
                .arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-File")
                .arg(path);
            return Ok(command);
        }

        #[cfg(not(windows))]
        {
            return Err(anyhow!(
                "Build script '{}' is a PowerShell script, which is only supported on Windows",
                path.display()
            ));
        }
    }

    Ok(Command::new(path))
}

fn review_script(path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read build script '{}'", path.display()))?;
    println!(
        "{}",
        output::title(format!("Reviewing script: {}", path.display()))
    );
    for line in content.lines() {
        println!("  {line}");
    }
    Ok(())
}

pub fn run_build_script(
    action: BuildScriptAction,
    workspace_root: &Path,
    line_callback: Option<&mut dyn FnMut(&str)>,
) -> Result<()> {
    let Some(path) = script_for(action, workspace_root) else {
        return Ok(());
    };

    validate_script(&path)?;
    review_script(&path)?;
    println!(
        "  {}",
        output::meta(format!("Command: {}", command_preview(&path)))
    );
    output::confirm_or_cancel(
        format!(
            "Run {} script '{}' from '{}' ?",
            action.label(),
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("script"),
            path.parent()
                .and_then(|value| value.file_name())
                .and_then(|value| value.to_str())
                .unwrap_or("scripts"),
        ),
        true,
    )?;

    let mut status_callback = line_callback;

    let mut command = command_for(&path)?;
    command.current_dir(workspace_root);

    let context = format!(
        "Failed to run build script '{}'. Check the script shebang, executable bit, and interpreter availability.",
        path.display()
    );
    let status =
        run_command_with_line_callback(&mut command, context.as_str(), &mut status_callback)
            .with_context(|| format!("Build script execution failed: '{}'", path.display()))?;

    if !status.success() {
        bail!(
            "Script '{}' exited with non-zero status ({})",
            path.display(),
            status.code().unwrap_or(-1)
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BuildScriptAction, is_ps1, script_for, validate_script};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, path::PathBuf};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-builder-script-test-{name}-{nanos}"))
    }

    #[test]
    fn install_prefers_root_bash_over_scripts_sh() {
        let root = temp_root("install-prefers-sh");
        fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
        fs::write(
            root.join("install.bash"),
            "#!/usr/bin/env bash\necho bash\n",
        )
        .expect("write root install.bash");
        fs::write(
            root.join("scripts").join("install.sh"),
            "#!/bin/sh\necho sh\n",
        )
        .expect("write scripts install.sh");
        let path = script_for(BuildScriptAction::Install, &root).expect("detect script");
        assert_eq!(path, root.join("install.bash"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn upgrade_prefers_upgrade_script_over_install() {
        let root = temp_root("upgrade-priority");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("install.sh"), "#!/bin/sh\necho install\n").expect("write install");
        fs::write(root.join("upgrade.bash"), "#!/bin/bash\necho upgrade\n").expect("write upgrade");
        let path = script_for(BuildScriptAction::Upgrade, &root).expect("detect script");
        assert_eq!(path, root.join("upgrade.bash"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn supports_scripts_directory_fallback() {
        let root = temp_root("scripts-fallback");
        fs::create_dir_all(root.join("scripts")).expect("create scripts dir");
        fs::write(
            root.join("scripts").join("install.bash"),
            "#!/bin/bash\necho scripts\n",
        )
        .expect("write scripts install.bash");
        let path = script_for(BuildScriptAction::Install, &root).expect("detect script");
        assert_eq!(path, root.join("scripts").join("install.bash"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn detects_ps1_candidates() {
        let root = temp_root("ps1-candidate");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("install.ps1"), "Write-Output install\n").expect("write ps1");
        let path = script_for(BuildScriptAction::Install, &root).expect("detect script");
        assert_eq!(path, root.join("install.ps1"));
        assert!(is_ps1(&path));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn non_ps1_script_requires_shebang() {
        let root = temp_root("requires-shebang");
        fs::create_dir_all(&root).expect("create root");
        let script = root.join("install.sh");
        fs::write(&script, "echo no shebang\n").expect("write script");
        let err = validate_script(&script).expect_err("must reject missing shebang");
        assert!(err.to_string().contains("missing a shebang"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn non_ps1_script_with_shebang_is_valid() {
        let root = temp_root("with-shebang");
        fs::create_dir_all(&root).expect("create root");
        let script = root.join("install.sh");
        fs::write(&script, "#!/bin/sh\necho ok\n").expect("write script");
        validate_script(&script).expect("valid shebang script");
        let _ = fs::remove_dir_all(&root);
    }
}
