use crate::{
    services::{
        packaging::{
            PackageInstaller,
            PackageRemover,
            PackageUpgrader,
            PackageChecker,
        },
        storage::package_storage::PackageStorage,
    },
    providers::provider_manager::ProviderManager,
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, anyhow};
use console::style;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct UpgradeOperation<'a> {
    upgrader: PackageUpgrader<'a>,
    checker: PackageChecker<'a>,
    package_storage: &'a mut PackageStorage,
}

impl<'a> UpgradeOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
    ) -> Result<Self> {
        let installer = PackageInstaller::new(provider_manager, paths)?;
        let remover = PackageRemover::new(paths);

        let upgrader = PackageUpgrader::new(
            provider_manager,
            installer,
            remover,
            paths,
        );

        let checker = PackageChecker::new(provider_manager);

        Ok(Self {
            upgrader,
            checker,
            package_storage,
        })
    }

    pub async fn upgrade_all<F, G, H>(
        &mut self,
        force_option: &bool,
        download_progress: &mut Option<F>,
        overall_progress: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let names: Vec<String> = self
            .package_storage
            .get_all_packages()
            .iter()
            .map(|p| p.name.clone())
            .collect();

        self.upgrade_bulk(
            &names,
            force_option,
            download_progress,
            overall_progress,
            message_callback,
        )
        .await
    }

    pub async fn upgrade_bulk<F, G, H>(
        &mut self,
        names: &Vec<String>,
        force_option: &bool,
        download_progress: &mut Option<F>,
        overall_progress: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let total = names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;
        let mut upgraded = 0;

        for name in names {
            message!(message_callback, "Checking '{}' ...", name);

            let package = self.package_storage
                .get_package_by_name(name)
                .ok_or_else(|| anyhow!("Package '{}' is not installed", name))?
                .clone();

            match self
                .upgrader
                .upgrade(
                    &package,
                    *force_option,
                    download_progress,
                    message_callback,
                )
                .await
                .context(format!("Failed to upgrade package '{}'", name))
            {
                Ok(Some(updated)) => {
                    self.package_storage.add_or_update_package(updated)?;
                    message!(
                        message_callback,
                        "{}",
                        style(format!("Package '{}' upgraded", name)).green()
                    );
                    upgraded += 1;
                }
                Ok(None) => {
                    message!(
                        message_callback,
                        "Package '{}' is already up to date",
                        name
                    );
                }
                Err(e) => {
                    message!(
                        message_callback,
                        "{} {}",
                        style(format!("Upgrade failed for '{}':", name)).red(),
                        e
                    );
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress.as_mut() {
                cb(completed, total);
            }
        }

        self.package_storage
            .save_packages()
            .context("Failed to save updated package information")?;

        message!(
            message_callback,
            "Completed: {} upgraded, {} up-to-date, {} failed",
            upgraded,
            total - upgraded - failures,
            failures
        );

        Ok(())
    }

    pub async fn upgrade_single<F, H>(
        &mut self,
        package_name: &str,
        force_option: &bool,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<bool>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let package = self.package_storage
            .get_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?
            .clone();

        let upgraded = self
            .upgrader
            .upgrade(
                &package,
                *force_option,
                download_progress,
                message_callback,
            )
            .await?;

        if let Some(updated) = upgraded {
            self.package_storage.add_or_update_package(updated)?;
            self.package_storage.save_packages()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn check_updates<H>(
        &self,
        message_callback: &mut Option<H>,
    ) -> Result<Vec<(String, String, String)>>
    where
        H: FnMut(&str),
    {
        let packages = self.package_storage.get_all_packages();
        let mut updates = Vec::new();

        for pkg in packages {
            message!(message_callback, "Checking '{}' ...", pkg.name);

            match self.checker.check_one(pkg).await {
                Ok(Some((current, latest))) => {
                    message!(
                        message_callback,
                        "{} {} → {}",
                        style(format!("Update available for '{}':", pkg.name)).green(),
                        current,
                        latest
                    );
                    updates.push((pkg.name.clone(), current, latest));
                }
                Ok(None) => {
                    message!(message_callback, "'{}' is up to date", pkg.name);
                }
                Err(e) => {
                    message!(
                        message_callback,
                        "{} {}",
                        style(format!("Failed to check '{}':", pkg.name)).red(),
                        e
                    );
                }
            }
        }

        Ok(updates)
    }

    pub async fn check_single_update<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<Option<(String, String)>>
    where
        H: FnMut(&str),
    {
        let package = self.package_storage
            .get_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

        message!(message_callback, "Checking '{}' ...", package.name);

        match self.checker.check_one(package).await? {
            Some((current, latest)) => {
                message!(
                    message_callback,
                    "{} {} → {}",
                    style("Update available:").green(),
                    current,
                    latest
                );
                Ok(Some((current, latest)))
            }
            None => {
                message!(message_callback, "'{}' is up to date", package.name);
                Ok(None)
            }
        }
    }
}

