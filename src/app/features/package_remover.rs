use std::fs;
use anyhow::{Result, anyhow};

use crate::models::upstream::{Package};
use crate::services::filesystem::{ShellIntegrator, SymlinkManager};
use crate::services::storage::package_storage::PackageStorage;
use crate::utils::static_paths::UpstreamPaths;

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
    pub fn new(
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
    ) -> Self {
        Self {
            package_storage,
            paths,
        }
    }

    pub fn remove_bulk<H, G>(
        &mut self,
        package_names: Vec<String>,
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

            match self.remove_single(&package_name, message_callback) {
                Ok(_) => message!(message_callback, "Package removed!"),
                Err(e) => {
                    message!(message_callback, "Removal failed: {}", e);
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        if failures > 0 {
            message!(message_callback, "{} package(s) failed to be removed", failures);
        }

        Ok(())
    }

    pub fn remove_single<H>(
        &mut self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let package = self.package_storage
            .get_mut_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed.", package_name))?;

        Self::perform_remove(self.paths, package, message_callback)?;
            self.package_storage.save_packages()?;
            Ok(())
        }

    fn perform_remove<H>(
        paths: &UpstreamPaths,
        package: &mut Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = package.install_path.as_ref()
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package.name))?;

        message!(message_callback, "Removing '{}' from PATH ...", install_path.display());

        ShellIntegrator::new(&paths.config.paths_file, &paths.integration.symlinks_dir).remove_from_paths(install_path)?;

        message!(message_callback, "Removing symlink for '{}'", package.name);

        SymlinkManager::new(&paths.integration.symlinks_dir).remove_link(&package.name)?;

        if install_path.is_dir() {
            message!(message_callback, "Removing directory: {}", install_path.display());
            fs::remove_dir_all(install_path)?;
        } else if install_path.is_file() {
            message!(message_callback, "Removing file: {}", install_path.display());
            fs::remove_file(install_path)?;
        } else {
            return Err(anyhow!("Install path is invalid: {}", install_path.display()));
        }

        package.install_path = None;
        package.exec_path = None;

        Ok(())
    }
}
