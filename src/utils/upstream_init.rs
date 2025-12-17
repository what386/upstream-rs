use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

// The line to source in shell configs
const SOURCE_LINE_BASH: &str = "[ -f $HOME/.config/upstream/paths.sh ] && source $HOME/.config/upstream/paths.sh";
const SOURCE_LINE_FISH: &str = "source $HOME/.config/upstream/paths.sh";

pub struct Paths {
    pub user_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub config_file: PathBuf,
    pub metadata_dir: PathBuf,
    pub packages_file: PathBuf,
    pub repositories_file: PathBuf,
    pub paths_file: PathBuf,
    pub appimages_dir: PathBuf,
    pub binaries_dir: PathBuf,
    pub archives_dir: PathBuf,
    pub symlinks_dir: PathBuf,
    pub scripts_dir: PathBuf,
    pub xdg_applications_dir: PathBuf,
    pub xdg_icons_dir: PathBuf,
}

impl Paths {
    fn new() -> Self {
        let user_dir = dirs::home_dir().expect("Unable to determine home directory");
        let config_dir = user_dir.join(".config/upstream");
        let data_dir = user_dir.join(".upstream");
        let metadata_dir = data_dir.join("metadata");

        Self {
            config_file: config_dir.join("config.json"),
            packages_file: metadata_dir.join("packages.json"),
            repositories_file: metadata_dir.join("repositories.json"),
            paths_file: metadata_dir.join("paths.sh"),
            appimages_dir: data_dir.join("appimages"),
            binaries_dir: data_dir.join("binaries"),
            archives_dir: data_dir.join("archives"),
            symlinks_dir: data_dir.join("symlinks"),
            scripts_dir: data_dir.join("scripts"),
            xdg_applications_dir: user_dir.join(".local/share/applications"),
            xdg_icons_dir: user_dir.join(".local/share/icons"),
            user_dir,
            config_dir,
            data_dir,
            metadata_dir,
        }
    }
}

pub const PATHS: LazyLock<Paths> = LazyLock::new(Paths::new);

pub fn initialize() -> io::Result<()> {
    create_package_dirs()?;
    create_metadata_files()?;
    update_shell_profiles()?;
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
    fs::create_dir_all(&PATHS.config_dir)?;
    fs::create_dir_all(&PATHS.data_dir)?;
    fs::create_dir_all(&PATHS.metadata_dir)?;
    fs::create_dir_all(&PATHS.appimages_dir)?;
    fs::create_dir_all(&PATHS.binaries_dir)?;
    fs::create_dir_all(&PATHS.archives_dir)?;
    fs::create_dir_all(&PATHS.symlinks_dir)?;
    Ok(())
}

fn create_metadata_files() -> io::Result<()> {
    if !PATHS.paths_file.exists() {
        fs::write(
            &PATHS.paths_file,
            "#!/bin/bash\n# Upstream managed PATH additions\n",
        )?;
    }
    Ok(())
}

fn update_shell_profiles() -> io::Result<()> {
    let shells = get_installed_shells()?;
    for shell_path in shells {
        let shell_name = Path::new(&shell_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        match shell_name.to_lowercase().as_str() {
            "bash" | "sh" => {
                add_line_to_profile(".bashrc", SOURCE_LINE_BASH)?;
            }
            "zsh" => {
                add_line_to_profile(".zshrc", SOURCE_LINE_BASH)?;
            }
            "fish" => {
                let fish_config = Path::new(".config").join("fish").join("config.fish");
                add_line_to_profile(fish_config.to_str().unwrap(), SOURCE_LINE_FISH)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn add_line_to_profile(relative_path: &str, line: &str) -> io::Result<()> {
    let profile_path = PATHS.user_dir.join(relative_path);
    // Ensure parent directory exists
    if let Some(parent) = profile_path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Backup original file
    if profile_path.exists() {
        let backup_path = profile_path.with_extension("bak");
        if !backup_path.exists() {
            fs::copy(&profile_path, &backup_path)?;
        }
    }

    if !profile_path.exists() {
        fs::write(&profile_path, format!("{}\n", line))?;
        return Ok(());
    }

    let content = fs::read_to_string(&profile_path)?;
    if !content.contains(line) {
        let mut file = fs::OpenOptions::new().append(true).open(&profile_path)?;
        writeln!(file, "\n{}", line)?;
    }
    Ok(())
}

pub fn cleanup() -> io::Result<()> {
    let shells = get_installed_shells()?;
    for shell_path in shells {
        let shell_name = Path::new(&shell_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let profile = match shell_name.to_lowercase().as_str() {
            "bash" | "sh" => Some(".bashrc"),
            "zsh" => Some(".zshrc"),
            "fish" => Some(".config/fish/config.fish"),
            _ => None,
        };
        if let Some(profile_rel) = profile {
            let profile_path = PATHS.user_dir.join(profile_rel);
            if !profile_path.exists() {
                continue;
            }
            let mut content = fs::read_to_string(&profile_path)?;
            content = content
                .replace(&format!("{}\n", SOURCE_LINE_BASH), "")
                .replace(SOURCE_LINE_BASH, "")
                .replace(&format!("{}\n", SOURCE_LINE_FISH), "")
                .replace(SOURCE_LINE_FISH, "");
            fs::write(&profile_path, content)?;
        }
    }
    Ok(())
}
