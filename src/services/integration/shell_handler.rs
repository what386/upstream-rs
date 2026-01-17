use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub struct ShellManager<'a> {
    paths_file: &'a Path,
}

impl<'a> ShellManager<'a> {
    pub fn new(paths_file: &'a Path) -> Self {
        Self { paths_file }
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
            let mut content =
                fs::read_to_string(self.paths_file).context("Failed to read paths file")?;
            let escaped = install_path
                .to_string_lossy()
                .replace('$', "\\$")
                .replace('"', "\\\"");
            let export_line = format!("export PATH=\"{escaped}:$PATH\"");

            if !content.contains(&export_line) {
                content.push_str(&format!("{export_line}\n"));
                fs::write(self.paths_file, &content).context("Failed to write paths file")?;
            }
        }

        #[cfg(windows)]
        {
            self.add_to_windows_registry(install_path)?;
        }

        Ok(())
    }

    /// Removes a package's PATH entry
    pub fn remove_from_paths(&self, install_path: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            let mut content =
                fs::read_to_string(self.paths_file).context("Failed to read paths file")?;
            let escaped = install_path
                .to_string_lossy()
                .replace('$', "\\$")
                .replace('"', "\\\"");
            let export_line = format!("export PATH=\"{escaped}:$PATH\"");

            content = content.replace(&format!("{export_line}\n"), "");
            content = content.replace(&export_line, "");
            fs::write(self.paths_file, content).context("Failed to write paths file")?;
        }

        #[cfg(windows)]
        {
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

        // Get current PATH
        let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

        // Check if path is already in PATH
        let path_entries: Vec<&str> = current_path.split(';').collect();
        if path_entries.iter().any(|&p| p.trim() == target_path) {
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

        // Get current PATH
        let current_path: String = env_key.get_value("Path").unwrap_or_else(|_| String::new());

        // Remove target path from PATH
        let path_entries: Vec<&str> = current_path
            .split(';')
            .filter(|&p| p.trim() != target_path)
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
