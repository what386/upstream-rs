#[cfg(unix)]
use crate::utils::filesystem::atomic_ops::write_atomic;
use anyhow::{Context, Result};
#[cfg(unix)]
use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::{Mutex, OnceLock};

/// Process-global lock used to serialize reads/writes to the shared PATH file.
#[cfg(unix)]
fn paths_file_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(windows)]
fn normalize_windows_path(path: &str) -> String {
    let mut normalized = path.replace('/', "\\").trim().to_ascii_lowercase();
    while normalized.ends_with('\\') {
        normalized.pop();
    }
    normalized
}

pub struct ShellManager<'a> {
    paths_file: &'a Path,
    #[cfg(unix)]
    paths_nu_file: PathBuf,
}

impl<'a> ShellManager<'a> {
    pub fn new(paths_file: &'a Path) -> Self {
        Self {
            paths_file,
            #[cfg(unix)]
            paths_nu_file: paths_file.with_extension("nu"),
        }
    }

    /// Adds a package's installation path to PATH
    pub fn add_to_paths(&self, install_path: &Path) -> Result<()> {
        if !install_path.is_dir() {
            anyhow::bail!(
                "Package install directory not found: {}",
                install_path.to_string_lossy()
            );
        }

        #[cfg(unix)]
        {
            let _guard = paths_file_lock()
                .lock()
                .ok()
                .ok_or_else(|| anyhow::anyhow!("Failed to lock PATH file for writing"))?;
            let mut content =
                fs::read_to_string(self.paths_file).context("Failed to read paths file")?;
            let escaped = install_path
                .to_string_lossy()
                .replace('$', "\\$")
                .replace('"', "\\\"");
            let export_line = format!("export PATH=\"{escaped}:$PATH\"");

            if !content.contains(&export_line) {
                content.push_str(&format!("{export_line}\n"));
                write_atomic(self.paths_file, content.as_bytes())
                    .context("Failed to write paths file")?;
            }

            let nushell_content = fs::read_to_string(&self.paths_nu_file).unwrap_or_default();
            let mut nushell_paths = parse_nushell_paths_file(&nushell_content);
            let install_path = install_path.to_string_lossy().to_string();

            if !nushell_paths.contains(&install_path) {
                nushell_paths.insert(0, install_path);
                let rendered = render_nushell_paths_file(&nushell_paths);
                write_atomic(&self.paths_nu_file, rendered.as_bytes())
                    .context("Failed to write Nushell paths file")?;
            }
        }

        #[cfg(windows)]
        {
            let _ = self.paths_file;
            self.add_to_windows_registry(install_path)?;
        }

        Ok(())
    }

    /// Removes a package's PATH entry
    pub fn remove_from_paths(&self, install_path: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            let _guard = paths_file_lock()
                .lock()
                .ok()
                .ok_or_else(|| anyhow::anyhow!("Failed to lock PATH file for writing"))?;
            let mut content =
                fs::read_to_string(self.paths_file).context("Failed to read paths file")?;
            let escaped = install_path
                .to_string_lossy()
                .replace('$', "\\$")
                .replace('"', "\\\"");
            let export_line = format!("export PATH=\"{escaped}:$PATH\"");

            content = content.replace(&format!("{export_line}\n"), "");
            content = content.replace(&export_line, "");
            write_atomic(self.paths_file, content.as_bytes())
                .context("Failed to write paths file")?;

            if self.paths_nu_file.exists() {
                let nushell_content = fs::read_to_string(&self.paths_nu_file)
                    .context("Failed to read Nushell paths file")?;
                let target = install_path.to_string_lossy().to_string();
                let mut nushell_paths = parse_nushell_paths_file(&nushell_content);
                let original_len = nushell_paths.len();
                nushell_paths.retain(|path| path != &target);

                if nushell_paths.len() != original_len {
                    let rendered = render_nushell_paths_file(&nushell_paths);
                    write_atomic(&self.paths_nu_file, rendered.as_bytes())
                        .context("Failed to write Nushell paths file")?;
                }
            }
        }

        #[cfg(windows)]
        {
            let _ = self.paths_file;
            self.remove_from_windows_registry(install_path)?;
        }

        Ok(())
    }

    #[cfg(windows)]
    fn add_to_windows_registry(&self, path: &Path) -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env_key = hkcu
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .context("Failed to open registry Environment key")?;

        let target_path = path.to_string_lossy().to_string();
        let target_norm = normalize_windows_path(&target_path);

        // Get current PATH
        let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

        // Check if path is already in PATH
        let path_entries: Vec<&str> = current_path.split(';').collect();
        if path_entries
            .iter()
            .any(|&p| normalize_windows_path(p) == target_norm)
        {
            return Ok(()); // Already in PATH
        }

        // Add path to the beginning
        let new_path = if current_path.is_empty() {
            target_path
        } else {
            format!("{};{}", target_path, current_path)
        };

        env_key
            .set_value("Path", &new_path)
            .context("Failed to set PATH in registry")?;

        // Broadcast environment change
        Self::broadcast_environment_change();

        Ok(())
    }

    #[cfg(windows)]
    fn remove_from_windows_registry(&self, path: &Path) -> Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env_key = hkcu
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .context("Failed to open registry Environment key")?;

        let target_path = path.to_string_lossy().to_string();
        let target_norm = normalize_windows_path(&target_path);

        // Get current PATH
        let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

        // Remove target path from PATH
        let path_entries: Vec<&str> = current_path
            .split(';')
            .filter(|&p| normalize_windows_path(p) != target_norm)
            .collect();

        let new_path = path_entries.join(";");

        env_key
            .set_value("Path", &new_path)
            .context("Failed to set PATH in registry")?;

        // Broadcast environment change
        Self::broadcast_environment_change();

        Ok(())
    }

    #[cfg(windows)]
    fn broadcast_environment_change() {
        use std::ptr;
        use winapi::shared::minwindef::LPARAM;
        use winapi::um::winuser::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        };

        unsafe {
            let env_string: Vec<u16> = "Environment\0".encode_utf16().collect();
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                env_string.as_ptr() as LPARAM,
                SMTO_ABORTIFHUNG,
                5000,
                ptr::null_mut(),
            );
        }
    }
}

#[cfg(unix)]
fn escape_nushell_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(unix)]
pub fn render_nushell_paths_file(paths: &[String]) -> String {
    let mut content = String::from("# Upstream managed PATH additions\n\nlet upstream_paths = [\n");
    for path in paths {
        content.push_str(&format!("    \"{}\"\n", escape_nushell_string(path)));
    }
    content.push_str("]\n\n$env.PATH = ($upstream_paths ++ $env.PATH)\n");
    content
}

#[cfg(unix)]
pub fn nushell_paths_file_contains_path(content: &str, path: &str) -> bool {
    parse_nushell_paths_file(content)
        .iter()
        .any(|entry| entry == path)
}

#[cfg(unix)]
fn parse_nushell_paths_file(content: &str) -> Vec<String> {
    let mut list_entries = Vec::new();
    let mut prepend_entries = Vec::new();
    let mut in_list = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "let upstream_paths = [" {
            in_list = true;
            continue;
        }
        if in_list && trimmed == "]" {
            in_list = false;
            continue;
        }

        if in_list {
            if let Some((path, _)) = parse_nushell_string_literal(trimmed) {
                list_entries.push(path);
            }
            continue;
        }

        if let Some(prepend_index) = trimmed.find("| prepend ") {
            let rest = trimmed[prepend_index + "| prepend ".len()..].trim_start();
            if let Some((path, _)) = parse_nushell_string_literal(rest) {
                prepend_entries.push(path);
            }
        }
    }

    if list_entries.is_empty() {
        prepend_entries.reverse();
        dedupe_preserving_order(prepend_entries)
    } else {
        prepend_entries.reverse();
        list_entries.extend(prepend_entries);
        dedupe_preserving_order(list_entries)
    }
}

#[cfg(unix)]
fn parse_nushell_string_literal(input: &str) -> Option<(String, usize)> {
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    if first != '"' {
        return None;
    }

    let mut output = String::new();
    let mut escaped = false;
    for (index, ch) in chars {
        if escaped {
            match ch {
                '"' | '\\' => output.push(ch),
                other => {
                    output.push('\\');
                    output.push(other);
                }
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some((output, index + ch.len_utf8())),
            other => output.push(other),
        }
    }

    None
}

#[cfg(unix)]
fn dedupe_preserving_order(paths: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for path in paths {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::{ShellManager, parse_nushell_paths_file};
    #[cfg(unix)]
    use std::path::{Path, PathBuf};
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(unix)]
    use std::{fs, io};

    #[cfg(unix)]
    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-shell-test-{name}-{nanos}"))
    }

    #[cfg(unix)]
    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[cfg(unix)]
    #[test]
    fn add_to_paths_is_idempotent_and_escapes_special_characters() {
        let root = temp_root("add-idempotent");
        let install_path = root.join("tool\"dir$");
        let paths_file = root.join("paths.sh");
        let paths_nu_file = root.join("paths.nu");
        fs::create_dir_all(&install_path).expect("create install dir");
        fs::write(&paths_file, "#!/usr/bin/env sh\n").expect("create paths file");
        fs::write(&paths_nu_file, "# Upstream managed PATH additions\n").expect("create paths.nu");
        let manager = ShellManager::new(&paths_file);

        manager.add_to_paths(&install_path).expect("first add");
        manager.add_to_paths(&install_path).expect("second add");

        let content = fs::read_to_string(&paths_file).expect("read paths file");
        assert_eq!(content.matches("export PATH=").count(), 1);
        assert!(content.contains("\\\""));
        assert!(content.contains("\\$"));

        let nushell_content = fs::read_to_string(&paths_nu_file).expect("read paths.nu");
        assert!(nushell_content.contains("let upstream_paths = ["));
        assert!(nushell_content.contains("$env.PATH = ($upstream_paths ++ $env.PATH)"));
        assert_eq!(
            parse_nushell_paths_file(&nushell_content),
            vec![install_path.to_string_lossy().to_string()]
        );
        assert!(nushell_content.contains("\\\""));
        assert!(nushell_content.contains("$"));

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn remove_from_paths_removes_existing_export_line() {
        let root = temp_root("remove");
        let install_path = root.join("pkg/bin");
        let paths_file = root.join("paths.sh");
        let paths_nu_file = root.join("paths.nu");
        fs::create_dir_all(&install_path).expect("create install dir");
        fs::write(&paths_file, "").expect("create paths file");
        fs::write(&paths_nu_file, "# Upstream managed PATH additions\n").expect("create paths.nu");
        let manager = ShellManager::new(&paths_file);

        manager.add_to_paths(&install_path).expect("add path");
        manager
            .remove_from_paths(&install_path)
            .expect("remove path");

        let content = fs::read_to_string(&paths_file).expect("read paths file");
        assert!(!content.contains("export PATH="));

        let nushell_content = fs::read_to_string(&paths_nu_file).expect("read paths.nu");
        assert!(parse_nushell_paths_file(&nushell_content).is_empty());

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn add_to_paths_migrates_old_nushell_prepend_lines_to_managed_list() {
        let root = temp_root("migrate-nu");
        let first_path = root.join("first/bin");
        let second_path = root.join("second/bin");
        let new_path = root.join("new/bin");
        let paths_file = root.join("paths.sh");
        let paths_nu_file = root.join("paths.nu");
        fs::create_dir_all(&first_path).expect("create first dir");
        fs::create_dir_all(&second_path).expect("create second dir");
        fs::create_dir_all(&new_path).expect("create new dir");
        fs::write(&paths_file, "#!/usr/bin/env sh\n").expect("create paths file");
        fs::write(
            &paths_nu_file,
            format!(
                "# Upstream managed PATH additions\n\
                 $env.PATH = ($env.PATH | prepend \"{}\")\n\
                 $env.PATH = ($env.PATH | prepend \"{}\")\n",
                first_path.display(),
                second_path.display()
            ),
        )
        .expect("create old paths.nu");
        let manager = ShellManager::new(&paths_file);

        manager.add_to_paths(&new_path).expect("add path");

        let nushell_content = fs::read_to_string(&paths_nu_file).expect("read paths.nu");
        assert!(!nushell_content.contains(" | prepend "));
        assert_eq!(
            parse_nushell_paths_file(&nushell_content),
            vec![
                new_path.to_string_lossy().to_string(),
                second_path.to_string_lossy().to_string(),
                first_path.to_string_lossy().to_string(),
            ]
        );

        cleanup(&root).expect("cleanup");
    }
}
