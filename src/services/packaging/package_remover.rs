use crate::{
    models::upstream::Package,
    services::filesystem::{DesktopManager, ShellManager, SymlinkManager},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use std::fs;

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

        message!(
            message_callback,
            "Removing '{}' from PATH ...",
            install_path.display()
        );

        ShellManager::new(&self.paths.config.paths_file, &self.paths.integration.symlinks_dir)
            .remove_from_paths(install_path)
            .context(format!(
                "Failed to remove '{}' from PATH configuration",
                install_path.display()
            ))?;

        message!(message_callback, "Removing symlink for '{}'", package.name);

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .remove_link(&package.name)
            .context(format!("Failed to remove symlink for '{}'", package.name))?;

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
            message!(message_callback, "Removing .desktop entry ...");

            let desktop_manager =
                DesktopManager::new(self.paths).context("Failed to initialize desktop manager")?;
            desktop_manager
                .remove_entry(&package.name)
                .context(format!(
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

    /// Purge configuration files for a package
    pub fn purge_configs<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        // TODO: implement
        // - Search for config directories matching package_name
        // - Prompt user or log which configs are being removed
        // - Add context for each removal operation
        message!(
            message_callback,
            "Purge option enabled, but configuration removal not yet implemented for '{}'",
            package_name
        );
        Ok(())
    }
}
