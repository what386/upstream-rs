use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::utils::static_paths::UpstreamPaths;

// The line to source in shell configs
const SOURCE_LINE_BASH: &str = "[ -f $HOME/.upstream/metadata/paths.sh ] && source $HOME/.config/upstream/metadata/paths.sh";
const SOURCE_LINE_FISH: &str = "source $HOME/.upstream/metadata/paths.sh";

pub fn initialize(paths: &UpstreamPaths) -> io::Result<()> {
    create_package_dirs(paths)?;
    create_metadata_files(paths)?;
    update_shell_profiles(paths)?;
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

fn create_package_dirs(paths: &UpstreamPaths) -> io::Result<()> {
    fs::create_dir_all(&paths.dirs.config_dir)?;
    fs::create_dir_all(&paths.dirs.data_dir)?;
    fs::create_dir_all(&paths.dirs.metadata_dir)?;
    fs::create_dir_all(&paths.install.appimages_dir)?;
    fs::create_dir_all(&paths.install.binaries_dir)?;
    fs::create_dir_all(&paths.install.archives_dir)?;
    fs::create_dir_all(&paths.integration.symlinks_dir)?;
    Ok(())
}

fn create_metadata_files(paths: &UpstreamPaths) -> io::Result<()> {
    if !paths.config.paths_file.exists() {
        fs::write(
            &paths.config.paths_file,
            "#!/bin/bash\n# Upstream managed PATH additions\n",
        )?;
    }
    Ok(())
}

fn update_shell_profiles(paths: &UpstreamPaths) -> io::Result<()> {
    let shells = get_installed_shells()?;
    for shell_path in shells {
        let shell_name = Path::new(&shell_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        match shell_name.to_lowercase().as_str() {
            "bash" | "sh" => {
                add_line_to_profile(paths, ".bashrc", SOURCE_LINE_BASH)?;
            }
            "zsh" => {
                add_line_to_profile(paths, ".zshrc", SOURCE_LINE_BASH)?;
            }
            "fish" => {
                let fish_config = Path::new(".config").join("fish").join("config.fish");
                add_line_to_profile(paths, fish_config.to_str().unwrap(), SOURCE_LINE_FISH)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn add_line_to_profile(paths: &UpstreamPaths, relative_path: &str, line: &str) -> io::Result<()> {
    let profile_path = paths.dirs.user_dir.join(relative_path);
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

pub fn cleanup(paths: &UpstreamPaths) -> io::Result<()> {
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
            let profile_path = paths.dirs.user_dir.join(profile_rel);
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
