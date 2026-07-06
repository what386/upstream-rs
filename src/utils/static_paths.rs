use anyhow::{Context, Result};
use std::path::PathBuf;

/// Root directories for the application
pub struct AppDirs {
    pub user_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub packages_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub metadata_dir: PathBuf,
}

impl Default for AppDirs {
    fn default() -> Self {
        Self::new().expect("failed to determine upstream app directories")
    }
}

impl AppDirs {
    pub fn new() -> Result<Self> {
        let user_dir =
            dirs::home_dir().context("Failed to determine the current user's home directory")?;
        let config_dir = dirs::config_dir()
            .context("Failed to determine the current user's config directory")?
            .join("upstream");

        let data_dir = user_dir.join(".upstream");
        let state_dir = data_dir.join("state");

        let packages_dir = data_dir.join("packages");
        let metadata_dir = data_dir.join("metadata");
        let cache_dir = data_dir.join("cache");

        Ok(Self {
            user_dir,
            config_dir,
            data_dir,
            state_dir,
            packages_dir,
            cache_dir,
            metadata_dir,
        })
    }
}

/// Paths to configuration and metadata files
pub struct ConfigPaths {
    pub config_file: PathBuf,
    pub auth_file: PathBuf,
    pub packages_file: PathBuf,
    pub packages_database_file: PathBuf,
    pub trust_file: PathBuf,
    pub paths_file: PathBuf,
    pub paths_nu_file: PathBuf,
}

impl ConfigPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            config_file: dirs.config_dir.join("config.toml"),
            auth_file: dirs.config_dir.join("auth.toml"),
            packages_file: dirs.metadata_dir.join("packages.json"),
            packages_database_file: dirs.metadata_dir.join("packages.db"),
            trust_file: dirs.metadata_dir.join("trust.json"),
            paths_file: dirs.metadata_dir.join("paths.sh"),
            paths_nu_file: dirs.metadata_dir.join("paths.nu"),
        }
    }
}

/// Directories where packages are installed
pub struct InstallPaths {
    pub appimages_dir: PathBuf,
    pub binaries_dir: PathBuf,
    pub archives_dir: PathBuf,
    pub tmp_dir: PathBuf,
}

impl InstallPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            appimages_dir: dirs.packages_dir.join("appimages"),
            binaries_dir: dirs.packages_dir.join("binaries"),
            archives_dir: dirs.packages_dir.join("archives"),
            tmp_dir: dirs.data_dir.join("temp"),
        }
    }
}

/// Paths for persistent app state.
pub struct StatePaths {
    pub rollback_dir: PathBuf,
    pub symlinks_dir: PathBuf,
    pub icons_dir: PathBuf,
}

impl StatePaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            rollback_dir: dirs.state_dir.join("rollback"),
            symlinks_dir: dirs.state_dir.join("symlinks"),
            icons_dir: dirs.state_dir.join("icons"),
        }
    }
}

/// Paths for system integration (XDG dirs and shell completions)
pub struct IntegrationPaths {
    pub xdg_applications_dir: PathBuf,
    pub bash_completions_dir: PathBuf,
    pub fish_completions_dir: PathBuf,
    pub zsh_completions_dir: PathBuf,
}

impl IntegrationPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
            bash_completions_dir: dirs
                .user_dir
                .join(".local/share/bash-completion/completions"),
            fish_completions_dir: dirs.user_dir.join(".config/fish/completions"),
            zsh_completions_dir: dirs.user_dir.join(".local/share/zsh/site-functions"),
        }
    }
}

/// Convenience wrapper that holds all path types
pub struct UpstreamPaths {
    pub dirs: AppDirs,
    pub config: ConfigPaths,
    pub install: InstallPaths,
    pub state: StatePaths,
    pub integration: IntegrationPaths,
}

impl Default for UpstreamPaths {
    fn default() -> Self {
        Self::new().expect("failed to determine upstream paths")
    }
}

impl UpstreamPaths {
    pub fn new() -> Result<Self> {
        let dirs = AppDirs::new()?;
        Ok(Self {
            config: ConfigPaths::new(&dirs),
            install: InstallPaths::new(&dirs),
            state: StatePaths::new(&dirs),
            integration: IntegrationPaths::new(&dirs),
            dirs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::UpstreamPaths;

    #[test]
    fn upstream_paths_are_composed_from_base_directories() {
        let paths = UpstreamPaths::new().expect("paths");

        assert_eq!(
            paths.config.config_file,
            paths.dirs.config_dir.join("config.toml")
        );
        assert_eq!(paths.config.auth_file, paths.dirs.config_dir.join("auth.toml"));
        assert_eq!(
            paths.config.packages_file,
            paths.dirs.metadata_dir.join("packages.json")
        );
        assert_eq!(
            paths.config.packages_database_file,
            paths.dirs.metadata_dir.join("packages.db")
        );
        assert_eq!(
            paths.config.trust_file,
            paths.dirs.metadata_dir.join("trust.json")
        );
        assert_eq!(
            paths.config.paths_nu_file,
            paths.dirs.metadata_dir.join("paths.nu")
        );
        assert_eq!(
            paths.install.binaries_dir,
            paths.dirs.packages_dir.join("binaries")
        );
        assert_eq!(
            paths.dirs.packages_dir,
            paths.dirs.data_dir.join("packages")
        );
        assert_eq!(paths.dirs.cache_dir, paths.dirs.data_dir.join("cache"));
        assert_eq!(paths.dirs.state_dir, paths.dirs.data_dir.join("state"));
        assert_eq!(
            paths.state.rollback_dir,
            paths.dirs.state_dir.join("rollback")
        );
        assert_eq!(
            paths.state.symlinks_dir,
            paths.dirs.state_dir.join("symlinks")
        );
        assert_eq!(paths.state.icons_dir, paths.dirs.state_dir.join("icons"));
        assert_eq!(
            paths.integration.fish_completions_dir,
            paths.dirs.user_dir.join(".config/fish/completions")
        );
        assert_eq!(paths.install.tmp_dir, paths.dirs.data_dir.join("temp"));
    }
}
