use crate::{
    services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
use crate::services::packaging::PackageRemover;
use anyhow::{Context, Result, anyhow};
use console::style;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct RemoveOperation<'a> {
    remover: PackageRemover<'a>,
    package_storage: &'a mut PackageStorage,
}

impl<'a> RemoveOperation<'a> {
    pub fn new(package_storage: &'a mut PackageStorage, paths: &'a UpstreamPaths) -> Self {
        let remover = PackageRemover::new(paths);
        Self {
            remover,
            package_storage,
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

        self.remover
            .remove_package_files(package, message_callback)
            .context(format!(
                "Failed to perform removal operations for '{}'",
                package_name
            ))?;

        self.package_storage
            .remove_package_by_name(package_name)
            .context(format!(
                "Failed to remove '{}' from package storage",
                package_name
            ))?;

        if *purge_option {
            self.remover
                .purge_configs(package_name, message_callback)
                .context(format!(
                    "Failed to purge configuration files for '{}'",
                    package_name
                ))?;
        }

        Ok(())
    }
}
