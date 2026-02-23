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
use futures_util::stream::{self, StreamExt};
use indicatif::HumanBytes;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration};

const CHECK_CONCURRENCY: usize = 8;
const UPGRADE_CONCURRENCY: usize = 4;

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
    fn truncate_error(value: &str, max: usize) -> String {
        let char_count = value.chars().count();
        if char_count <= max {
            return value.to_string();
        }

        let mut out = String::new();
        for ch in value.chars().take(max.saturating_sub(3)) {
            out.push(ch);
        }
        out.push_str("...");
        out
    }

    fn format_transfer(downloaded: u64, total: u64) -> String {
        if total > 0 {
            format!("{} / {}", HumanBytes(downloaded), HumanBytes(total))
        } else if downloaded > 0 {
            format!("{}", HumanBytes(downloaded))
        } else {
            "-".to_string()
        }
    }

    fn render_progress_row(
        name: &str,
        channel: &Channel,
        provider: &Provider,
        downloaded: u64,
        total: u64,
    ) -> String {
        format!(
            " {:<28} {:<10} {:<3} {:<10} {}",
            name,
            channel.to_string().to_lowercase(),
            "u",
            provider.to_string(),
            Self::format_transfer(downloaded, total)
        )
    }

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
        ignore_checksums: bool,
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
            ignore_checksums,
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
        ignore_checksums: bool,
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
        let force = *force_option;
        let upgrader = &self.upgrader;
        let progress_state: Arc<Mutex<BTreeMap<String, (Channel, Provider, u64, u64)>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let mut last_progress_render: BTreeMap<String, String> = BTreeMap::new();

        let packages: Vec<_> = names
            .iter()
            .map(|name| {
                self.package_storage
                    .get_package_by_name(name)
                    .ok_or_else(|| anyhow!("Package '{}' is not installed", name))
                    .cloned()
            })
            .collect::<Result<Vec<_>>>()?;

        let mut updated_packages = Vec::new();
        let mut pending = stream::iter(packages.into_iter().map(|package| {
            let state_ref = Arc::clone(&progress_state);
            async move {
                let name = package.name.clone();
                let channel = package.channel.clone();
                let provider = package.provider.clone();

                if let Ok(mut state) = state_ref.lock() {
                    state.insert(name.clone(), (channel.clone(), provider.clone(), 0, 0));
                }

                let mut downloaded: u64 = 0;
                let mut bytes_total: u64 = 0;
                let mut download_cb = Some(|d: u64, t: u64| {
                    downloaded = d;
                    bytes_total = t;
                    if let Ok(mut state) = state_ref.lock() {
                        state.insert(name.clone(), (channel.clone(), provider.clone(), d, t));
                    }
                });
                let mut no_messages: Option<fn(&str)> = None;

                let result = upgrader
                    .upgrade(
                        &package,
                        force,
                        ignore_checksums,
                        &mut download_cb,
                        &mut no_messages,
                    )
                    .await
                    .context(format!("Failed to upgrade package '{}'", name));
                (name, channel, provider, downloaded, bytes_total, result)
            }
        }))
        .buffer_unordered(UPGRADE_CONCURRENCY);

        let mut ticker = time::interval(Duration::from_millis(350));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        while completed < total {
            tokio::select! {
                maybe_item = pending.next() => {
                    let Some((name, channel, provider, downloaded, bytes_total, result)) = maybe_item else {
                        break;
                    };

                    if let Ok(mut state) = progress_state.lock() {
                        state.remove(&name);
                    }
                    last_progress_render.remove(&name);
                    message!(message_callback, "__UPGRADE_PROGRESS_DONE__ {}", name);

            let transfer = Self::format_transfer(downloaded, bytes_total);
            match result {
                Ok(Some(updated)) => {
                    updated_packages.push(updated);
                    message!(
                        message_callback,
                        "[✓] {:<28} {:<10} {:<3} {:<10} {}",
                        name,
                        channel.to_string().to_lowercase(),
                        "u",
                        provider.to_string(),
                        transfer
                    );
                    upgraded += 1;
                }
                Ok(None) => {
                    message!(
                        message_callback,
                        "[=] {:<28} {:<10} {:<3} {:<10} {}",
                        name,
                        channel.to_string().to_lowercase(),
                        "-",
                        provider.to_string(),
                        transfer
                    );
                }
                Err(e) => {
                    message!(
                        message_callback,
                        "[!] {:<28} {:<10} {:<3} {:<10} {}",
                        name,
                        channel.to_string().to_lowercase(),
                        "!",
                        provider.to_string(),
                        Self::truncate_error(&e.to_string(), 36)
                    );
                    failures += 1;
                }
            }

                    completed += 1;
                    if let Some(cb) = overall_progress.as_mut() {
                        cb(completed, total);
                    }
                }
                _ = ticker.tick() => {
                    if let Ok(state) = progress_state.lock() {
                        for (name, (channel, provider, downloaded, total_bytes)) in state.iter() {
                            let row = Self::render_progress_row(
                                name,
                                channel,
                                provider,
                                *downloaded,
                                *total_bytes
                            );
                            let changed = last_progress_render
                                .get(name)
                                .map(|prev| prev != &row)
                                .unwrap_or(true);
                            if changed {
                                message!(message_callback, "__UPGRADE_PROGRESS_ROW__ {}\t{}", name, row);
                                last_progress_render.insert(name.clone(), row);
                            }
                        }
                    }
                }
            }
        }

        message!(message_callback, "__UPGRADE_PROGRESS_CLEAR__");

        // Save storage updates once parallel workers are done.
        for updated in updated_packages {
            self.package_storage.add_or_update_package(updated)?;
        }

        // Bulk mode uses per-package workers; a single shared download progress bar is noisy.
        let _ = download_progress;

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
        ignore_checksums: bool,
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
            .upgrade(
                &package,
                *force_option,
                ignore_checksums,
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

    pub async fn check_all_detailed(&self) -> Vec<UpdateCheckRow> {
        let packages = self.package_storage.get_all_packages().to_vec();
        self.check_installed_packages_detailed(packages).await
    }

    pub async fn check_all_machine_readable(&self) -> Vec<(String, String, String)> {
        let rows = self.check_all_detailed().await;
        rows.into_iter()
            .filter_map(|row| match row.status {
                UpdateCheckStatus::UpdateAvailable { current, latest } => {
                    Some((row.name, current, latest))
                }
                _ => None,
            })
            .collect()
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

    pub async fn check_selected_machine_readable(
        &self,
        package_names: &[String],
    ) -> Vec<(String, String, String)> {
        let rows = self.check_selected_detailed(package_names).await;
        rows.into_iter()
            .filter_map(|row| match row.status {
                UpdateCheckStatus::UpdateAvailable { current, latest } => {
                    Some((row.name, current, latest))
                }
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "../../../tests/application/operations/upgrade_operation.rs"]
mod tests;
