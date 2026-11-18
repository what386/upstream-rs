use anyhow::{Result, anyhow, bail};
use std::path::Path;

use crate::models::common::DesktopEntry;

/// Validates launch options and formats desktop launch commands.
pub struct LauncherManager;

impl LauncherManager {
    pub fn require_support() -> Result<()> {
        if !cfg!(target_os = "linux") {
            bail!("Desktop launch options are currently supported only on Linux");
        }
        Ok(())
    }

    pub fn validate(template: &str) -> Result<Vec<String>> {
        if template.contains(['\0', '\n', '\r']) {
            bail!("desktop_launch_options cannot contain NUL bytes or newlines");
        }
        let arguments = shlex::split(template)
            .ok_or_else(|| anyhow!("desktop_launch_options contains invalid argument quoting"))?;
        if arguments.iter().filter(|arg| arg.as_str() == "{}").count() != 1
            || arguments
                .iter()
                .any(|arg| arg != "{}" && arg.contains("{}"))
        {
            bail!("desktop_launch_options must contain exactly one standalone {{}} argument");
        }
        if arguments
            .first()
            .is_none_or(|arg| arg.is_empty() || arg.contains('='))
        {
            bail!(
                "desktop_launch_options must start with a command; use env KEY=value for environment variables"
            );
        }
        Ok(arguments)
    }

    /// Render literal arguments using desktop-entry escaping, never shell quoting.
    /// Existing desktop arguments (including field codes) are appended by the caller.
    pub fn format_desktop_command(arguments: &[String], executable: &Path) -> Result<String> {
        let executable = executable
            .to_str()
            .ok_or_else(|| anyhow!("Executable path is not valid UTF-8"))?;
        if executable.contains(['\0', '\n', '\r', '=']) {
            bail!("Executable path cannot be represented in a desktop launch command");
        }
        Ok(arguments
            .iter()
            .map(|arg| {
                DesktopEntry::quote_exec_argument(if arg == "{}" { executable } else { arg })
            })
            .collect::<Vec<_>>()
            .join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_templates() {
        for template in [
            "",
            "tool --flag",
            "{} {}",
            "prefix{}",
            "{} '{}'suffix",
            "{} 'unterminated",
            "'' {}",
            "FOO=bar {}",
            "{}\0",
            "{}\nName=bad",
        ] {
            assert!(LauncherManager::validate(template).is_err(), "{template:?}");
        }
    }

    #[test]
    fn renders_desktop_arguments_with_literal_percent_and_shell_characters() {
        let options =
            LauncherManager::validate("env FOO=bar {} --profile 'Work Profile' '$HOME' '100%' ''")
                .unwrap();
        assert_eq!(
            LauncherManager::format_desktop_command(&options, Path::new("/my apps/tool")).unwrap(),
            "env FOO=bar \"/my apps/tool\" --profile \"Work Profile\" \"\\\\$HOME\" 100%% \"\""
        );
    }
}
