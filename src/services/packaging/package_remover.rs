use crate::{
    models::upstream::Package,
    services::integration::{DesktopManager, ShellManager, SymlinkManager},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use dirs;
use std::fs;
use std::path::Path;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct PackageRemover<'a> {
    paths: &'a UpstreamPaths,
}

impl<'a> PackageRemover<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    /// Remove package files and integrations
    pub fn remove_package_files<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        self.remove_runtime_integrations(package, message_callback)?;

        if install_path.is_dir() {
            message!(
                message_callback,
                "Removing directory: {}",
                install_path.display()
            );
            fs::remove_dir_all(install_path).context(format!(
                "Failed to remove installation directory at '{}'",
                install_path.display()
            ))?;
        } else if install_path.is_file() {
            message!(
                message_callback,
                "Removing file: {}",
                install_path.display()
            );
            fs::remove_file(install_path).context(format!(
                "Failed to remove installation file at '{}'",
                install_path.display()
            ))?;
        } else {
            return Err(anyhow!(
                "Install path '{}' is neither a file nor directory (may have been manually removed)",
                install_path.display()
            ));
        }

        if let Some(icon_path) = &package.icon_path {
            message!(message_callback, "Removing desktop entry ...");
            DesktopManager::remove_entry(self.paths, &package.name).context(format!(
                "Failed to remove desktop entry for '{}'",
                package.name
            ))?;

            fs::remove_file(icon_path).context(format!(
                "Failed to remove icon file at '{}'",
                icon_path.display()
            ))?;
            message!(
                message_callback,
                "Removed stored icon: {}",
                icon_path.display()
            );
        }

        Ok(())
    }

    /// Remove PATH and symlink state for a package without deleting installed files.
    pub fn remove_runtime_integrations<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        message!(
            message_callback,
            "Removing '{}' from PATH ...",
            install_path.display()
        );

        ShellManager::new(&self.paths.config.paths_file)
            .remove_from_paths(install_path)
            .context(format!(
                "Failed to remove '{}' from PATH configuration",
                install_path.display()
            ))?;

        message!(message_callback, "Removing symlink for '{}'", package.name);
        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .remove_link(&package.name)
            .context(format!("Failed to remove symlink for '{}'", package.name))?;

        Ok(())
    }

    /// Restore PATH and symlink state for a previously installed package.
    pub fn restore_runtime_integrations<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        if install_path.is_dir() {
            message!(
                message_callback,
                "Restoring '{}' to PATH ...",
                install_path.display()
            );
            ShellManager::new(&self.paths.config.paths_file)
                .add_to_paths(install_path)
                .context(format!(
                    "Failed to restore '{}' in PATH configuration",
                    install_path.display()
                ))?;
        }

        if let Some(exec_path) = package.exec_path.as_ref()
            && exec_path.exists()
        {
            message!(message_callback, "Restoring symlink for '{}'", package.name);
            SymlinkManager::new(&self.paths.integration.symlinks_dir)
                .add_link(exec_path, &package.name)
                .context(format!("Failed to restore symlink for '{}'", package.name))?;
        }

        Ok(())
    }

    /// Purge configuration files for a package
    pub fn purge_configs<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        message!(
            message_callback,
            "Purging package data for '{}' ...",
            package_name
        );

        // Remove known upstream-owned integration artifacts by package alias.
        DesktopManager::remove_entry(self.paths, package_name).context(format!(
            "Failed to remove desktop entry for '{}'",
            package_name
        ))?;
        self.remove_matching_icons(package_name, message_callback)?;

        // Best-effort XDG/user-dir cleanup for app-owned state.
        let mut candidates = Vec::new();
        if let Some(config_dir) = dirs::config_dir() {
            candidates.push(config_dir.join(package_name));
            candidates.push(config_dir.join(package_name.to_lowercase()));
        }
        if let Some(cache_dir) = dirs::cache_dir() {
            candidates.push(cache_dir.join(package_name));
            candidates.push(cache_dir.join(package_name.to_lowercase()));
        }
        if let Some(data_dir) = dirs::data_local_dir() {
            candidates.push(data_dir.join(package_name));
            candidates.push(data_dir.join(package_name.to_lowercase()));
        }

        // Dedup while preserving order.
        let mut unique = Vec::new();
        for path in candidates {
            if !unique.contains(&path) {
                unique.push(path);
            }
        }

        for path in unique {
            self.remove_path_if_exists(&path, message_callback)?;
        }

        Ok(())
    }

    fn remove_matching_icons<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let icons_dir = &self.paths.integration.icons_dir;
        if !icons_dir.exists() {
            return Ok(());
        }

        let package_name_lower = package_name.to_lowercase();
        for entry in fs::read_dir(icons_dir).context(format!(
            "Failed to read icons directory '{}'",
            icons_dir.display()
        ))? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if stem == package_name_lower {
                self.remove_path_if_exists(&path, message_callback)?;
            }
        }

        Ok(())
    }

    fn remove_path_if_exists<H>(&self, path: &Path, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        if !path.exists() {
            return Ok(());
        }

        if path.is_dir() {
            message!(message_callback, "Purging directory: {}", path.display());
            fs::remove_dir_all(path)
                .context(format!("Failed to remove directory '{}'", path.display()))?;
        } else if path.is_file() {
            message!(message_callback, "Purging file: {}", path.display());
            fs::remove_file(path).context(format!("Failed to remove file '{}'", path.display()))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::PackageRemover;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::utils::static_paths::{
        AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths,
    };
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-remover-test-{name}-{nanos}"))
    }

    fn test_paths(root: &Path) -> UpstreamPaths {
        let dirs = AppDirs {
            user_dir: root.to_path_buf(),
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            metadata_dir: root.join("data/metadata"),
        };

        UpstreamPaths {
            config: ConfigPaths {
                config_file: dirs.config_dir.join("config.toml"),
                packages_file: dirs.metadata_dir.join("packages.json"),
                paths_file: dirs.metadata_dir.join("paths.sh"),
            },
            install: InstallPaths {
                appimages_dir: dirs.data_dir.join("appimages"),
                binaries_dir: dirs.data_dir.join("binaries"),
                archives_dir: dirs.data_dir.join("archives"),
            },
            integration: IntegrationPaths {
                symlinks_dir: dirs.data_dir.join("symlinks"),
                xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
                icons_dir: dirs.data_dir.join("icons"),
            },
            dirs,
        }
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn remove_path_if_exists_deletes_file_and_directory() {
        let root = temp_root("remove-path");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.integration.icons_dir).expect("create icons dir");
        let file = paths.integration.icons_dir.join("pkg.png");
        fs::write(&file, b"icon").expect("write icon");
        let nested_dir = root.join("to-remove");
        fs::create_dir_all(nested_dir.join("x")).expect("create nested dir");

        let remover = PackageRemover::new(&paths);
        let mut messages: Option<fn(&str)> = None;
        remover
            .remove_path_if_exists(&file, &mut messages)
            .expect("remove file");
        remover
            .remove_path_if_exists(&nested_dir, &mut messages)
            .expect("remove directory");
        remover
            .remove_path_if_exists(&root.join("missing"), &mut messages)
            .expect("ignore missing");

        assert!(!file.exists());
        assert!(!nested_dir.exists());

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn remove_runtime_integrations_requires_install_path() {
        let root = temp_root("runtime-missing-path");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.paths_file.parent().expect("parent"))
            .expect("create metadata dir");
        fs::write(&paths.config.paths_file, "").expect("create paths file");

        let package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        let remover = PackageRemover::new(&paths);
        let mut messages: Option<fn(&str)> = None;

        let err = remover
            .remove_runtime_integrations(&package, &mut messages)
            .expect_err("must fail without install path");
        assert!(err.to_string().contains("no install path"));

        cleanup(&root).expect("cleanup");
    }
}
