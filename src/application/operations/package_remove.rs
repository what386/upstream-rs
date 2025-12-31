use crate::{
    models::upstream::Package,
    services::{
        filesystem::{DesktopManager, ShellManager, SymlinkManager},
        storage::package_storage::PackageStorage,
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use console::style;
use std::fs;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct PackageRemover<'a> {
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
}

impl<'a> PackageRemover<'a> {
    pub fn new(package_storage: &'a mut PackageStorage, paths: &'a UpstreamPaths) -> Self {
        Self {
            package_storage,
            paths,
        }
    }

    pub fn remove_bulk<H, G>(
        &mut self,
        package_names: &Vec<String>,
        purge_option: &bool,
        message_callback: &mut Option<H>,
        overall_progress_callback: &mut Option<G>,
    ) -> Result<()>
    where
        H: FnMut(&str),
        G: FnMut(u32, u32),
    {
        let total = package_names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;

        for package_name in package_names {
            message!(message_callback, "Removing '{}' ...", package_name);

            match self
                .remove_single(package_name, purge_option, message_callback)
                .context(format!("Failed to remove package '{}'", package_name))
            {
                Ok(_) => message!(message_callback, "{}", style("Package removed").green()),
                Err(e) => {
                    message!(message_callback, "{} {}", style("Removal failed:").red(), e);
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        if failures > 0 {
            message!(
                message_callback,
                "{} package(s) failed to be removed",
                failures
            );
        }

        Ok(())
    }

    pub fn remove_single<H>(
        &mut self,
        package_name: &str,
        purge_option: &bool,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let package = self
            .package_storage
            .get_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

        Self::perform_remove(self.paths, package, message_callback)
            .context(format!("Failed to perform removal operations for '{}'", package_name))?;

        self.package_storage
            .remove_package_by_name(package_name)
            .context(format!("Failed to remove '{}' from package storage", package_name))?;

        if *purge_option {
            Self::purge_configs(self.paths, package_name, message_callback)
                .context(format!("Failed to purge configuration files for '{}'", package_name))?;
        }

        Ok(())
    }

    fn perform_remove<H>(
        paths: &UpstreamPaths,
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

        ShellManager::new(&paths.config.paths_file, &paths.integration.symlinks_dir)
            .remove_from_paths(install_path)
            .context(format!(
                "Failed to remove '{}' from PATH configuration",
                install_path.display()
            ))?;

        message!(message_callback, "Removing symlink for '{}'", package.name);

        SymlinkManager::new(&paths.integration.symlinks_dir)
            .remove_link(&package.name)
            .context(format!("Failed to remove symlink for '{}'", package.name))?;

        if install_path.is_dir() {
            message!(
                message_callback,
                "Removing directory: {}",
                install_path.display()
            );
            fs::remove_dir_all(install_path)
                .context(format!(
                    "Failed to remove installation directory at '{}'",
                    install_path.display()
                ))?;
        } else if install_path.is_file() {
            message!(
                message_callback,
                "Removing file: {}",
                install_path.display()
            );
            fs::remove_file(install_path)
                .context(format!(
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

            let desktop_manager = DesktopManager::new(paths)
                .context("Failed to initialize desktop manager")?;

            desktop_manager
                .remove_entry(&package.name)
                .context(format!("Failed to remove desktop entry for '{}'", package.name))?;

            fs::remove_file(icon_path)
                .context(format!(
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

    fn purge_configs<H>(
        paths: &UpstreamPaths,
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
