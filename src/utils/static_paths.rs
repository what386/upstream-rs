use anyhow::{Context, Result};
use std::path::PathBuf;

/// Root directories for the application
pub struct AppDirs {
    pub user_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
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
        let metadata_dir = data_dir.join("metadata");

        Ok(Self {
            user_dir,
            config_dir,
            data_dir,
            metadata_dir,
        })
    }
}

/// Paths to configuration and metadata files
pub struct ConfigPaths {
    pub config_file: PathBuf,
    pub packages_file: PathBuf,
    pub metadata_file: PathBuf,
    pub paths_file: PathBuf,
}

impl ConfigPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            config_file: dirs.config_dir.join("config.toml"),
            packages_file: dirs.metadata_dir.join("packages.json"),
            metadata_file: dirs.metadata_dir.join("metadata.json"),
            paths_file: dirs.metadata_dir.join("paths.sh"),
        }
    }
}

/// Directories where packages are installed
pub struct InstallPaths {
    pub appimages_dir: PathBuf,
    pub binaries_dir: PathBuf,
    pub archives_dir: PathBuf,
}

impl InstallPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            appimages_dir: dirs.data_dir.join("appimages"),
            binaries_dir: dirs.data_dir.join("binaries"),
            archives_dir: dirs.data_dir.join("archives"),
        }
    }
}

/// Paths for system integration (symlinks, XDG dirs)
pub struct IntegrationPaths {
    pub symlinks_dir: PathBuf,
    pub xdg_applications_dir: PathBuf,
    pub icons_dir: PathBuf,
}

impl IntegrationPaths {
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            symlinks_dir: dirs.data_dir.join("symlinks"),
            icons_dir: dirs.data_dir.join("icons"),
            xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
        }
    }
}

/// Convenience wrapper that holds all path types
pub struct UpstreamPaths {
    pub dirs: AppDirs,
    pub config: ConfigPaths,
    pub install: InstallPaths,
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
        assert_eq!(
            paths.config.packages_file,
            paths.dirs.metadata_dir.join("packages.json")
        );
        assert_eq!(
            paths.config.metadata_file,
            paths.dirs.metadata_dir.join("metadata.json")
        );
        assert_eq!(
            paths.install.binaries_dir,
            paths.dirs.data_dir.join("binaries")
        );
        assert_eq!(
            paths.integration.symlinks_dir,
            paths.dirs.data_dir.join("symlinks")
        );
    }
}
