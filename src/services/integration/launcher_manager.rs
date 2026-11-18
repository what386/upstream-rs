#[cfg(windows)]
use anyhow::Context;
use anyhow::{Result, anyhow, bail};
use std::path::Path;

use crate::models::common::DesktopEntry;
#[cfg(windows)]
use crate::utils::platform::windows::quoting::{quote_windows_argument, split_windows_arguments};

/// Validates launch options and formats desktop launch commands.
pub struct LauncherManager;

impl LauncherManager {
    pub fn validate(template: &str) -> Result<Vec<String>> {
        if template.contains(['\0', '\n', '\r']) {
            bail!("desktop_launch_options cannot contain NUL bytes or newlines");
        }
        #[cfg(windows)]
        let arguments = split_windows_arguments(template)
            .context("desktop_launch_options contains invalid argument quoting")?;
        #[cfg(not(windows))]
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
                "desktop_launch_options must start with a command, not an environment assignment"
            );
        }
        Ok(arguments)
    }

    /// A Windows shortcut stores its target separately from its argument command line.
    #[cfg(windows)]
    pub fn format_windows_command(
        arguments: &[String],
        executable: &Path,
    ) -> Result<(String, String)> {
        let executable = executable
            .to_str()
            .ok_or_else(|| anyhow!("Executable path is not valid UTF-8"))?;
        if executable.contains(['\0', '\n', '\r']) {
            bail!("Executable path cannot be represented in a shortcut");
        }
        let mut expanded = arguments.iter().map(|argument| {
            if argument == "{}" {
                executable
            } else {
                argument.as_str()
            }
        });
        let target = expanded
            .next()
            .filter(|target| !target.is_empty())
            .ok_or_else(|| anyhow!("Desktop launch command is empty"))?;
        Ok((
            target.to_string(),
            expanded
                .map(quote_windows_argument)
                .collect::<Vec<_>>()
                .join(" "),
        ))
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

    #[cfg(windows)]
    #[test]
    fn windows_command_separates_prefix_target_and_quotes_app_arguments() {
        let arguments = split_windows_arguments(
            r#"C:\Tools\runner.exe --mode desktop {} --profile "Work Profile" "" "C:\my folder\\""#,
        )
        .unwrap();
        let executable = Path::new(r"C:\Apps\O'Brien\my app.exe");
        let (target, command_line) =
            LauncherManager::format_windows_command(&arguments, executable).unwrap();
        assert_eq!(target, r"C:\Tools\runner.exe");
        assert_eq!(
            split_windows_arguments(&command_line).unwrap(),
            [
                "--mode",
                "desktop",
                executable.to_str().unwrap(),
                "--profile",
                "Work Profile",
                "",
                r"C:\my folder\"
            ]
        );
        let direct =
            LauncherManager::format_windows_command(&["{}".into(), "--flag".into()], executable)
                .unwrap();
        assert_eq!(
            direct,
            (executable.to_string_lossy().into_owned(), "--flag".into())
        );
    }

    #[test]
    fn rejects_invalid_templates() {
        for template in [
            "",
            "tool --flag",
            "{} {}",
            "prefix{}",
            "{} '{}'suffix",
            "{} \"unterminated",
            "\"\" {}",
            "FOO=bar {}",
            "{}\0",
            "{}\nName=bad",
        ] {
            assert!(LauncherManager::validate(template).is_err(), "{template:?}");
        }
    }

    #[test]
    fn renders_desktop_arguments_with_literal_percent_and_shell_characters() {
        let options = LauncherManager::validate(
            r#"env FOO=bar {} --profile "Work Profile" "$HOME" "100%" """#,
        )
        .unwrap();
        assert_eq!(
            LauncherManager::format_desktop_command(&options, Path::new("/my apps/tool")).unwrap(),
            "env FOO=bar \"/my apps/tool\" --profile \"Work Profile\" \"\\\\$HOME\" 100%% \"\""
        );
    }
}
