use crate::{
    models::common::enums::{Channel, Provider},
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{PackageChecker, PackageInstaller, PackageRemover, PackageUpgrader},
        storage::package_storage::PackageStorage,
    },
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, anyhow};
use console::style;
use futures_util::stream::{self, StreamExt};

const CHECK_CONCURRENCY: usize = 8;

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

pub enum UpdateCheckStatus {
    UpdateAvailable { current: String, latest: String },
    UpToDate { current: String },
    Failed { error: String },
    NotInstalled,
}

pub struct UpdateCheckRow {
    pub name: String,
    pub channel: Option<Channel>,
    pub provider: Option<Provider>,
    pub status: UpdateCheckStatus,
}

impl<'a> UpgradeOperation<'a> {
    async fn check_packages_parallel(
        &self,
        packages: Vec<crate::models::upstream::Package>,
    ) -> Vec<(
        crate::models::upstream::Package,
        Result<Option<(String, String)>>,
    )> {
        let mut checked = stream::iter(packages.into_iter().enumerate().map(
            |(idx, pkg)| async move {
                let result = self.checker.check_one(&pkg).await;
                (idx, pkg, result)
            },
        ))
        .buffer_unordered(CHECK_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;

        checked.sort_by_key(|(idx, _, _)| *idx);
        checked
            .into_iter()
            .map(|(_, pkg, result)| (pkg, result))
            .collect()
    }

    async fn check_installed_packages_detailed(
        &self,
        packages: Vec<crate::models::upstream::Package>,
    ) -> Vec<UpdateCheckRow> {
        self.check_packages_parallel(packages)
            .await
            .into_iter()
            .map(|(pkg, result)| match result {
                Ok(Some((current, latest))) => UpdateCheckRow {
                    name: pkg.name,
                    channel: Some(pkg.channel),
                    provider: Some(pkg.provider),
                    status: UpdateCheckStatus::UpdateAvailable { current, latest },
                },
                Ok(None) => UpdateCheckRow {
                    name: pkg.name,
                    channel: Some(pkg.channel),
                    provider: Some(pkg.provider),
                    status: UpdateCheckStatus::UpToDate {
                        current: pkg.version.to_string(),
                    },
                },
                Err(error) => UpdateCheckRow {
                    name: pkg.name,
                    channel: Some(pkg.channel),
                    provider: Some(pkg.provider),
                    status: UpdateCheckStatus::Failed {
                        error: error.to_string(),
                    },
                },
            })
            .collect()
    }

    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
    ) -> Result<Self> {
        let installer = PackageInstaller::new(provider_manager, paths)?;
        let remover = PackageRemover::new(paths);

        let upgrader = PackageUpgrader::new(provider_manager, installer, remover, paths);

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

            let package = self
                .package_storage
                .get_package_by_name(name)
                .ok_or_else(|| anyhow!("Package '{}' is not installed", name))?
                .clone();

            match self
                .upgrader
                .upgrade(&package, *force_option, download_progress, message_callback)
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
                    message!(message_callback, "Package '{}' is already up to date", name);
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
        let package = self
            .package_storage
            .get_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?
            .clone();

        let upgraded = self
            .upgrader
            .upgrade(&package, *force_option, download_progress, message_callback)
            .await?;

        if let Some(updated) = upgraded {
            self.package_storage.add_or_update_package(updated)?;
            self.package_storage.save_packages()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn check_all_detailed(&self) -> Vec<UpdateCheckRow> {
        let packages = self.package_storage.get_all_packages().to_vec();
        self.check_installed_packages_detailed(packages).await
    }

    pub async fn check_selected_detailed(&self, package_names: &[String]) -> Vec<UpdateCheckRow> {
        let mut rows: Vec<Option<UpdateCheckRow>> =
            (0..package_names.len()).map(|_| None).collect();
        let mut selected_packages = Vec::new();
        let mut selected_indices = Vec::new();

        for (idx, name) in package_names.iter().enumerate() {
            match self.package_storage.get_package_by_name(name) {
                Some(package) => {
                    selected_packages.push(package.clone());
                    selected_indices.push(idx);
                }
                None => {
                    rows[idx] = Some(UpdateCheckRow {
                        name: name.clone(),
                        channel: None,
                        provider: None,
                        status: UpdateCheckStatus::NotInstalled,
                    })
                }
            }
        }

        let checked_rows = self
            .check_installed_packages_detailed(selected_packages)
            .await;
        for (row_idx, checked_row) in selected_indices.into_iter().zip(checked_rows) {
            rows[row_idx] = Some(checked_row);
        }

        rows.into_iter().flatten().collect()
    }
}
