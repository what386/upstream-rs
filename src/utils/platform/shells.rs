use std::env;
#[cfg(unix)]
use std::fs;
use std::path::{Path, PathBuf};

const SUPPORTED_SHELLS: &[&str] = &["bash", "sh", "zsh", "fish", "nu"];

pub fn installed_shell_commands() -> Vec<String> {
    let mut shells = Vec::new();

    #[cfg(unix)]
    {
        add_shells_from_etc_shells(&mut shells);
    }

    let path_value = env::var_os("PATH").unwrap_or_default();
    let path_dirs: Vec<PathBuf> = env::split_paths(&path_value).collect();
    add_shells_from_path(&mut shells, &path_dirs);

    shells
}

#[cfg(unix)]
fn add_shells_from_etc_shells(shells: &mut Vec<String>) {
    const SHELLS_FILE: &str = "/etc/shells";
    let Ok(content) = fs::read_to_string(SHELLS_FILE) else {
        return;
    };

    for shell in shell_commands_from_paths(content.lines()) {
        push_unique(shells, shell);
    }
}

fn add_shells_from_path(shells: &mut Vec<String>, path_dirs: &[PathBuf]) {
    for shell in SUPPORTED_SHELLS {
        if shells.iter().any(|existing| existing == shell) {
            continue;
        }

        if shell_exists_in_path(shell, path_dirs) {
            shells.push((*shell).to_string());
        }
    }
}

fn shell_exists_in_path(shell: &str, path_dirs: &[PathBuf]) -> bool {
    path_dirs.iter().any(|dir| {
        if dir.join(shell).is_file() {
            return true;
        }

        #[cfg(windows)]
        {
            if dir.join(format!("{shell}.exe")).is_file() {
                return true;
            }
        }

        false
    })
}

#[cfg(unix)]
fn shell_commands_from_paths<'a>(shell_paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut shells = Vec::new();
    for shell_path in shell_paths {
        let trimmed = shell_path.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let shell = Path::new(trimmed)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if SUPPORTED_SHELLS.contains(&shell) {
            push_unique(&mut shells, shell.to_string());
        }
    }
    shells
}

fn push_unique(shells: &mut Vec<String>, shell: String) {
    if !shells.contains(&shell) {
        shells.push(shell);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::shell_commands_from_paths;
    use super::{add_shells_from_path, shell_exists_in_path};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-shell-discovery-test-{name}-{nanos}"))
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn parses_supported_shell_commands_from_shell_paths() {
        let shells = shell_commands_from_paths([
            "# comment",
            "/bin/bash",
            "/usr/bin/fish",
            "/usr/bin/zsh",
            "/usr/bin/nu",
            "/usr/local/bin/bash",
            "/usr/bin/python",
            "",
        ]);

        assert_eq!(shells, vec!["bash", "fish", "zsh", "nu"]);
    }

    #[test]
    fn finds_shell_commands_from_path_dirs() {
        let root = temp_root("path");
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create bin dir");
        fs::write(bin.join("fish"), "").expect("create fish command");

        assert!(shell_exists_in_path("fish", &[bin]));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn path_discovery_does_not_duplicate_existing_shells() {
        let root = temp_root("dedupe");
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create bin dir");
        fs::write(bin.join("bash"), "").expect("create bash command");
        fs::write(bin.join("nu"), "").expect("create nu command");
        let mut shells = vec!["bash".to_string()];

        add_shells_from_path(&mut shells, &[bin]);

        assert_eq!(shells, vec!["bash".to_string(), "nu".to_string()]);

        cleanup(&root).expect("cleanup");
    }
}
