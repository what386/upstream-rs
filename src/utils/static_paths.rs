use anyhow::{Context, Result};
use std::path::PathBuf;

/// Root directories for the application
pub struct AppDirs {
    pub user_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub generated_dir: PathBuf,
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
        let generated_dir = data_dir.join("generated");
        let state_dir = data_dir.join("state");

        let packages_dir = data_dir.join("packages");
        let metadata_dir = data_dir.join("metadata");
        let cache_dir = data_dir.join("cache");

        Ok(Self {
            user_dir,
            config_dir,
            data_dir,
            generated_dir,
            state_dir,
            packages_dir,
            cache_dir,
            metadata_dir,
        })
    }
}

/// Paths to the application configuration file.
pub struct ConfigPaths {
    pub config_file: PathBuf,
}

/// Paths to persisted metadata and storage files.
pub struct MetadataPaths {
    pub auth_file: PathBuf,
    pub packages_file: PathBuf,
    pub packages_database_file: PathBuf,
    pub trust_file: PathBuf,
}

/// Paths to generated shell integration files.
pub struct GeneratedPaths {
    pub paths_file: PathBuf,
    pub paths_nu_file: PathBuf,
}

impl ConfigPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        let preferred_config = dirs.data_dir.join("config.toml");
        let fallback_config = dirs.config_dir.join("config.toml");

        let config_file = if preferred_config.is_file() {
            preferred_config
        } else if fallback_config.is_file() {
            fallback_config
        } else {
            // Neither exists yet- use the preferred location for creation.
            preferred_config
        };

        Self { config_file }
    }
}

impl MetadataPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            auth_file: dirs.metadata_dir.join("auth.toml"),
            packages_file: dirs.metadata_dir.join("packages.json"),
            packages_database_file: dirs.metadata_dir.join("packages.db"),
            trust_file: dirs.metadata_dir.join("trust.json"),
        }
    }
}

impl GeneratedPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            paths_file: dirs.generated_dir.join("paths.sh"),
            paths_nu_file: dirs.generated_dir.join("paths.nu"),
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
    pub symlinks_dir: PathBuf,
    pub icons_dir: PathBuf,
    pub lock_file: PathBuf,
}

impl StatePaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            symlinks_dir: dirs.state_dir.join("symlinks"),
            icons_dir: dirs.state_dir.join("icons"),
            lock_file: dirs.state_dir.join("lock"),
        }
    }
}

/// Paths for system integration (XDG dirs and shell completions)
pub struct IntegrationPaths {
    #[cfg(windows)]
    pub windows_programs_dir: PathBuf,
    pub xdg_data_dir: PathBuf,
    pub xdg_config_dir: PathBuf,
    pub xdg_applications_dir: PathBuf,
    pub user_units_dir: PathBuf,
    pub bash_completions_dir: PathBuf,
    pub fish_completions_dir: PathBuf,
    pub zsh_completions_dir: PathBuf,
    pub nushell_completions_dir: PathBuf,
}

impl IntegrationPaths {
    pub fn new(dirs: &AppDirs) -> Result<Self> {
        let data_dir = dirs::data_dir().unwrap_or_else(|| dirs.user_dir.join(".local/share"));
        let config_dir = dirs::config_dir().unwrap_or_else(|| dirs.user_dir.join(".config"));
        let mut paths = Self::from_xdg_dirs(&data_dir, &config_dir);
        #[cfg(windows)]
        {
            paths.windows_programs_dir =
                crate::utils::platform::windows::known_folders::programs_dir()
                    .context("Failed to determine the current user's Start menu Programs folder")?;
        }
        // Nushell honors XDG_DATA_HOME on every platform, including macOS and Windows.
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from)
            && data_home.is_absolute()
        {
            paths.nushell_completions_dir = data_home.join("nushell/vendor/autoload");
        }
        Ok(paths)
    }

    pub fn from_xdg_dirs(data_dir: &std::path::Path, config_dir: &std::path::Path) -> Self {
        Self {
            #[cfg(windows)]
            windows_programs_dir: data_dir.join("windows/programs"),
            xdg_data_dir: data_dir.to_path_buf(),
            xdg_config_dir: config_dir.to_path_buf(),
            xdg_applications_dir: data_dir.join("applications"),
            user_units_dir: data_dir.join("systemd/user"),
            bash_completions_dir: data_dir.join("bash-completion/completions"),
            fish_completions_dir: config_dir.join("fish/completions"),
            zsh_completions_dir: data_dir.join("zsh/site-functions"),
            nushell_completions_dir: data_dir.join("nushell/vendor/autoload"),
        }
    }
}

/// Convenience wrapper that holds all path types
pub struct UpstreamPaths {
    pub dirs: AppDirs,
    pub config: ConfigPaths,
    pub metadata: MetadataPaths,
    pub generated: GeneratedPaths,
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
            metadata: MetadataPaths::new(&dirs),
            generated: GeneratedPaths::new(&dirs),
            install: InstallPaths::new(&dirs),
            state: StatePaths::new(&dirs),
            integration: IntegrationPaths::new(&dirs)?,
            dirs,
        })
    }
}
