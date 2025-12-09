use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct UpstreamPaths;

impl UpstreamPaths {
    // The line to source in shell configs
    const SOURCE_LINE_BASH: &'static str =
        "[ -f $HOME/.config/upstream/paths.sh ] && source $HOME/.config/upstream/paths.sh";
    const SOURCE_LINE_FISH: &'static str =
        "source $HOME/.config/upstream/paths.sh";

    // Base directories - follow XDG spec where applicable
    fn user_dir() -> PathBuf {
        dirs::home_dir().expect("Unable to determine home directory")
    }

    pub fn config_dir() -> PathBuf {
        Self::user_dir().join(".config").join("upstream")
    }

    pub fn data_dir() -> PathBuf {
        Self::user_dir().join(".upstream")
    }

    // Config files (in ~/.config/upstream/)
    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    // Metadata (in ~/.upstream/metadata/)
    pub fn metadata_dir() -> PathBuf {
        Self::data_dir().join("metadata")
    }

    pub fn packages_file() -> PathBuf {
        Self::metadata_dir().join("packages.json")
    }

    pub fn repositories_file() -> PathBuf {
        Self::metadata_dir().join("repositories.json")
    }

    pub fn paths_file() -> PathBuf {
        Self::metadata_dir().join("paths.sh")
    }

    // Package storage (in ~/.upstream/)
    pub fn app_images_dir() -> PathBuf {
        Self::data_dir().join("appimages")
    }

    pub fn binaries_dir() -> PathBuf {
        Self::data_dir().join("binaries")
    }

    pub fn archives_dir() -> PathBuf {
        Self::data_dir().join("archives")
    }

    pub fn symlinks_dir() -> PathBuf {
        Self::data_dir().join("symlinks")
    }

    pub fn scripts_dir() -> PathBuf {
        Self::data_dir().join("scripts")
    }

    // XDG directories
    pub fn xdg_applications_dir() -> PathBuf {
        Self::user_dir().join(".local").join("share").join("applications")
    }

    pub fn xdg_icons_dir() -> PathBuf {
        Self::user_dir().join(".local").join("share").join("icons")
    }

    pub fn initialize() -> io::Result<()> {
        Self::create_package_dirs()?;
        Self::create_metadata_files()?;
        Self::update_shell_profiles()?;
        Ok(())
    }

    fn get_installed_shells() -> io::Result<Vec<String>> {
        const SHELLS_FILE: &str = "/etc/shells";

        if !Path::new(SHELLS_FILE).exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(SHELLS_FILE)?;
        let shells = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect();

        Ok(shells)
    }

    fn create_package_dirs() -> io::Result<()> {
        fs::create_dir_all(Self::config_dir())?;
        fs::create_dir_all(Self::data_dir())?;
        fs::create_dir_all(Self::metadata_dir())?;
        fs::create_dir_all(Self::app_images_dir())?;
        fs::create_dir_all(Self::binaries_dir())?;
        fs::create_dir_all(Self::archives_dir())?;
        fs::create_dir_all(Self::symlinks_dir())?;
        Ok(())
    }

    fn create_metadata_files() -> io::Result<()> {
        let paths_file = Self::paths_file();
        if !paths_file.exists() {
            fs::write(paths_file, "#!/bin/bash\n# Upstream managed PATH additions\n")?;
        }
        Ok(())
    }

    fn update_shell_profiles() -> io::Result<()> {
        let shells = Self::get_installed_shells()?;

        for shell_path in shells {
            let shell_name = Path::new(&shell_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            match shell_name.to_lowercase().as_str() {
                "bash" | "sh" => {
                    Self::add_line_to_profile(".bashrc", Self::SOURCE_LINE_BASH)?;
                }
                "zsh" => {
                    Self::add_line_to_profile(".zshrc", Self::SOURCE_LINE_BASH)?;
                }
                "fish" => {
                    let fish_config = Path::new(".config").join("fish").join("config.fish");
                    Self::add_line_to_profile(
                        fish_config.to_str().unwrap(),
                        Self::SOURCE_LINE_FISH
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn add_line_to_profile(relative_path: &str, line: &str) -> io::Result<()> {
        let profile_path = Self::user_dir().join(relative_path);

        // Ensure parent directory exists
        if let Some(parent) = profile_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Backup original file
        if profile_path.exists() {
            let backup_path = profile_path.with_extension("bak");
            if !backup_path.exists() {
                fs::copy(&profile_path, backup_path)?;
            }
        }

        if !profile_path.exists() {
            fs::write(&profile_path, format!("{}\n", line))?;
            return Ok(());
        }

        let content = fs::read_to_string(&profile_path)?;
        if !content.contains(line) {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&profile_path)?;
            writeln!(file, "\n{}", line)?;
        }

        Ok(())
    }

    pub fn cleanup() -> io::Result<()> {
        let shells = Self::get_installed_shells()?;

        for shell_path in shells {
            let shell_name = Path::new(&shell_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            let profile = match shell_name.to_lowercase().as_str() {
                "bash" | "sh" => Some(".bashrc"),
                "zsh" => Some(".zshrc"),
                "fish" => Some(".config/fish/config.fish"),
                _ => None
            };

            if let Some(profile_rel) = profile {
                let profile_path = Self::user_dir().join(profile_rel);

                if !profile_path.exists() {
                    continue;
                }

                let mut content = fs::read_to_string(&profile_path)?;
                content = content
                    .replace(&format!("{}\n", Self::SOURCE_LINE_BASH), "")
                    .replace(Self::SOURCE_LINE_BASH, "")
                    .replace(&format!("{}\n", Self::SOURCE_LINE_FISH), "")
                    .replace(Self::SOURCE_LINE_FISH, "");

                fs::write(&profile_path, content)?;
            }
        }
        Ok(())
    }
}
