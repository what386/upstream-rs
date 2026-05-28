use crate::{
    models::common::enums::{Channel, Provider, TrustMode},
    models::provider::Release,
    providers::provider_manager::ProviderManager,
    services::packaging::disk_impact::{
        ByteEstimate, DiskImpact, SignedByteEstimate, asset_size_estimate,
    },
    services::{
        packaging::{
            PackageChecker, PackageInstaller, PackageRemover, PackageUpgrader,
            ResolvedUpgradeTarget,
        },
        storage::package_storage::PackageStorage,
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, anyhow};
use futures_util::{
    future::{FutureExt, LocalBoxFuture},
    stream::{self, FuturesUnordered, StreamExt},
};
use indicatif::HumanBytes;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration};

const CHECK_CONCURRENCY: usize = 8;
const UPGRADE_CONCURRENCY: usize = 4;
type ProgressEntry = (Channel, Provider, u64, u64);
type ProgressState = Arc<Mutex<BTreeMap<String, ProgressEntry>>>;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

fn build_ref_version(label: impl AsRef<str>, commit: Option<&str>) -> String {
    let label = label.as_ref();
    let Some(commit) = commit else {
        return label.to_string();
    };
    let short: String = commit.chars().take(7).collect();
    format!("{label}@{short}")
}

pub struct UpgradeOperation<'a> {
    upgrader: PackageUpgrader<'a>,
    checker: PackageChecker<'a>,
    provider_manager: &'a ProviderManager,
    paths: &'a UpstreamPaths,
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

#[derive(Clone)]
pub struct UpgradePreviewRow {
    pub name: String,
    pub source: String,
    pub old_version: String,
    pub new_version: String,
    pub disk_impact: DiskImpact,
    pub target: ResolvedUpgradeTarget,
}

pub enum UpgradePreviewEvent {
    Started { package_width: usize },
    Checking { name: String },
    Row(UpgradePreviewRow),
}

impl<'a> UpgradeOperation<'a> {
    fn record_download_progress(
        progress_state: &ProgressState,
        name: &str,
        channel: &Channel,
        provider: &Provider,
        downloaded: u64,
        total: u64,
    ) {
        let Ok(mut state) = progress_state.lock() else {
            return;
        };

        if total > 0 && downloaded >= total {
            state.remove(name);
            return;
        }

        state.insert(
            name.to_string(),
            (channel.clone(), provider.clone(), downloaded, total),
        );
    }

    fn emit_progress_updates<H>(
        progress_state: &ProgressState,
        last_progress_render: &mut BTreeMap<String, String>,
        message_callback: &mut Option<H>,
    ) where
        H: FnMut(&str),
    {
        let snapshot = progress_state
            .lock()
            .map(|state| {
                state
                    .iter()
                    .map(|(name, entry)| (name.clone(), entry.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();

        for (name, (channel, provider, downloaded, total_bytes)) in &snapshot {
            let row = Self::render_progress_row(name, channel, provider, *downloaded, *total_bytes);
            let changed = last_progress_render
                .get(name)
                .map(|prev| prev != &row)
                .unwrap_or(true);
            if changed {
                message!(
                    message_callback,
                    "__UPGRADE_PROGRESS_ROW__ {}\t{}",
                    name,
                    row
                );
                last_progress_render.insert(name.clone(), row);
            }
        }

        let stale_names = last_progress_render
            .keys()
            .filter(|name| !snapshot.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in stale_names {
            last_progress_render.remove(&name);
            message!(message_callback, "__UPGRADE_PROGRESS_DONE__ {}", name);
        }
    }

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
        trusted_keys: TrustedSignatureKeys,
    ) -> Result<Self> {
        let installer = PackageInstaller::new(provider_manager, paths)?;
        let remover = PackageRemover::new(paths);

        let upgrader =
            PackageUpgrader::new(provider_manager, installer, remover, paths, trusted_keys);

        let checker = PackageChecker::new(provider_manager);

        Ok(Self {
            upgrader,
            checker,
            provider_manager,
            paths,
            package_storage,
        })
    }

    pub async fn estimate_upgrade_impact(
        &self,
        names: Option<&[String]>,
        force: bool,
    ) -> DiskImpact {
        let mut no_messages: Option<fn(&str)> = None;
        let Ok(rows) = self
            .preview_upgrade_with_messages(names, force, &mut no_messages)
            .await
        else {
            return DiskImpact::unknown();
        };
        rows.into_iter()
            .fold(DiskImpact::empty(), |total, row| total + row.disk_impact)
    }

    pub async fn preview_upgrade(
        &self,
        names: Option<&[String]>,
        force: bool,
    ) -> Result<Vec<UpgradePreviewRow>> {
        let mut no_messages: Option<fn(&str)> = None;
        self.preview_upgrade_with_messages(names, force, &mut no_messages)
            .await
    }

    pub async fn preview_upgrade_with_messages<H>(
        &self,
        names: Option<&[String]>,
        force: bool,
        message_callback: &mut Option<H>,
    ) -> Result<Vec<UpgradePreviewRow>>
    where
        H: FnMut(&str),
    {
        self.preview_upgrade_with_events(names, force, &mut |event| match event {
            UpgradePreviewEvent::Checking { name } => {
                message!(message_callback, "checking for updates: {}", name);
            }
            UpgradePreviewEvent::Started { .. } | UpgradePreviewEvent::Row(_) => {}
        })
        .await
    }

    pub async fn preview_upgrade_with_events<H>(
        &self,
        names: Option<&[String]>,
        force: bool,
        event_callback: &mut H,
    ) -> Result<Vec<UpgradePreviewRow>>
    where
        H: FnMut(UpgradePreviewEvent),
    {
        let packages = match names {
            Some(names) => names
                .iter()
                .map(|name| {
                    self.package_storage
                        .get_package_by_name(name)
                        .ok_or_else(|| anyhow!("Package '{}' is not installed", name))
                        .cloned()
                })
                .collect::<Result<Vec<_>>>()?,
            None => self.package_storage.get_all_packages().to_vec(),
        };
        let package_width = packages
            .iter()
            .map(|package| {
                format!(
                    "{}/{}",
                    package.channel.to_string().to_lowercase(),
                    package.name
                )
                .chars()
                .count()
            })
            .max()
            .unwrap_or("Package".len());
        event_callback(UpgradePreviewEvent::Started { package_width });

        let package_count = packages.len();
        let mut rows_by_index: Vec<Option<UpgradePreviewRow>> =
            (0..package_count).map(|_| None).collect();
        let mut package_iter = packages.into_iter().enumerate();
        let mut pending: FuturesUnordered<LocalBoxFuture<'_, (usize, Option<UpgradePreviewRow>)>> =
            FuturesUnordered::new();

        for _ in 0..CHECK_CONCURRENCY {
            let Some((idx, package)) = package_iter.next() else {
                break;
            };
            event_callback(UpgradePreviewEvent::Checking {
                name: package.name.clone(),
            });
            pending.push(
                async move { (idx, self.preview_package_upgrade(package, force).await) }
                    .boxed_local(),
            );
        }

        while let Some((idx, row)) = pending.next().await {
            if let Some(row) = row.clone() {
                event_callback(UpgradePreviewEvent::Row(row));
            }
            rows_by_index[idx] = row;

            if let Some((next_idx, next_package)) = package_iter.next() {
                event_callback(UpgradePreviewEvent::Checking {
                    name: next_package.name.clone(),
                });
                pending.push(
                    async move {
                        (
                            next_idx,
                            self.preview_package_upgrade(next_package, force).await,
                        )
                    }
                    .boxed_local(),
                );
            }
        }

        Ok(rows_by_index.into_iter().flatten().collect())
    }

    async fn preview_package_upgrade(
        &self,
        package: crate::models::upstream::Package,
        force: bool,
    ) -> Option<UpgradePreviewRow> {
        if package.is_pinned {
            return None;
        }

        if package.install_type == crate::models::upstream::InstallType::Build {
            if let Some(branch) = package.build_branch.as_deref() {
                let head_commit = self
                    .provider_manager
                    .get_branch_head_sha(
                        &package.repo_slug,
                        &package.provider,
                        branch,
                        package.base_url.as_deref(),
                    )
                    .await
                    .ok()?;
                let up_to_date = package
                    .build_commit
                    .as_deref()
                    .is_some_and(|saved| saved == head_commit);
                if up_to_date && !force {
                    return None;
                }

                return Some(UpgradePreviewRow {
                    name: package.name.clone(),
                    source: package.channel.to_string().to_lowercase(),
                    old_version: build_ref_version(
                        package.version.to_string(),
                        package.build_commit.as_deref(),
                    ),
                    new_version: build_ref_version(branch, Some(&head_commit)),
                    disk_impact: DiskImpact::unknown(),
                    target: ResolvedUpgradeTarget::Branch {
                        branch: branch.to_string(),
                        head_commit,
                    },
                });
            }
        }

        let release = if force {
            self.provider_manager
                .get_latest_release(
                    &package.repo_slug,
                    &package.provider,
                    &package.channel,
                    package.base_url.as_deref(),
                )
                .await
                .ok()
        } else {
            self.provider_manager
                .check_for_updates(&package)
                .await
                .ok()
                .flatten()
        }?;

        if !force {
            let up_to_date = if package.channel == Channel::Nightly {
                release.published_at <= package.last_upgraded
            } else {
                !release.version.is_newer_than(&package.version)
            };
            if up_to_date {
                return None;
            }
        }

        Some(UpgradePreviewRow {
            name: package.name.clone(),
            source: package.channel.to_string().to_lowercase(),
            old_version: package.version.to_string(),
            new_version: release.version.to_string(),
            disk_impact: if package.install_type == crate::models::upstream::InstallType::Build {
                DiskImpact::unknown()
            } else {
                self.estimate_release_upgrade_impact(&package, &release)
            },
            target: ResolvedUpgradeTarget::Release(release),
        })
    }

    fn estimate_release_upgrade_impact(
        &self,
        package: &crate::models::upstream::Package,
        release: &Release,
    ) -> DiskImpact {
        let Ok(asset) = self
            .provider_manager
            .find_recommended_asset(release, package)
        else {
            return DiskImpact::unknown();
        };

        let new_size = asset_size_estimate(asset.size);
        let old_rollback = crate::services::packaging::disk_impact::estimate_path_size(
            &self.paths.install.rollback_dir.join(&package.name),
        )
        .unwrap_or(0);
        match new_size.bytes {
            Some(bytes) => DiskImpact {
                download: new_size,
                net: SignedByteEstimate::estimated(
                    i128::from(bytes).saturating_sub(i128::from(old_rollback)),
                ),
            },
            None => DiskImpact {
                download: ByteEstimate::unknown(),
                net: SignedByteEstimate::unknown(),
            },
        }
    }

    pub async fn upgrade_all<F, G, H>(
        &mut self,
        force_option: &bool,
        trust_mode: TrustMode,
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
            trust_mode,
            download_progress,
            overall_progress,
            message_callback,
        )
        .await
    }

    pub async fn upgrade_bulk<F, G, H>(
        &mut self,
        names: &[String],
        force_option: &bool,
        trust_mode: TrustMode,
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
        let progress_state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
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

                let mut downloaded: u64 = 0;
                let mut bytes_total: u64 = 0;
                let mut download_cb = Some(|d: u64, t: u64| {
                    downloaded = d;
                    bytes_total = t;
                    Self::record_download_progress(&state_ref, &name, &channel, &provider, d, t);
                });
                let mut no_messages: Option<fn(&str)> = None;

                let result = upgrader
                    .upgrade(
                        &package,
                        force,
                        trust_mode,
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
                        "[ok] {:<28} {:<10} {:<3} {:<10} {}",
                        name,
                        channel.to_string().to_lowercase(),
                        "u",
                        provider.to_string(),
                        transfer
                    );
                    upgraded += 1;
                }
                Ok(None) => {}
                Err(e) => {
                    message!(
                        message_callback,
                        "[fail] {:<28} {:<10} {:<3} {:<10} {}",
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
                    Self::emit_progress_updates(&progress_state, &mut last_progress_render, message_callback);
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

    pub async fn upgrade_resolved_bulk<F, G, H>(
        &mut self,
        rows: &[UpgradePreviewRow],
        trust_mode: TrustMode,
        download_progress: &mut Option<F>,
        overall_progress: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<(u32, u32)>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let total = rows.len() as u32;
        let upgrader = &self.upgrader;
        let packages = rows
            .iter()
            .map(|row| {
                let package = self
                    .package_storage
                    .get_package_by_name(&row.name)
                    .ok_or_else(|| anyhow!("Package '{}' is not installed", row.name))?
                    .clone();
                Ok((package, row.clone()))
            })
            .collect::<Result<Vec<_>>>()?;

        let mut completed = 0;
        let mut upgraded = 0;
        let mut failures = 0;
        let mut updated_packages = Vec::new();
        let progress_state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let mut last_progress_render: BTreeMap<String, String> = BTreeMap::new();
        let mut pending = stream::iter(packages.into_iter().map(|(package, row)| {
            let state_ref = Arc::clone(&progress_state);
            async move {
                let name = package.name.clone();
                let channel = package.channel.clone();
                let provider = package.provider.clone();
                let new_version = row.new_version.clone();

                let mut downloaded: u64 = 0;
                let mut bytes_total: u64 = 0;
                let mut download_cb = Some(|d: u64, t: u64| {
                    downloaded = d;
                    bytes_total = t;
                    Self::record_download_progress(&state_ref, &name, &channel, &provider, d, t);
                });
                let mut no_messages: Option<fn(&str)> = None;

                let result = upgrader
                    .upgrade_resolved(
                        &package,
                        row.target,
                        trust_mode,
                        &mut download_cb,
                        &mut no_messages,
                    )
                    .await
                    .context(format!("Failed to upgrade package '{}'", name));

                (name, new_version, downloaded, bytes_total, result)
            }
        }))
        .buffer_unordered(UPGRADE_CONCURRENCY);

        let mut ticker = time::interval(Duration::from_millis(350));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        while completed < total {
            tokio::select! {
                maybe_item = pending.next() => {
                    let Some((name, new_version, downloaded, bytes_total, result)) = maybe_item else {
                        break;
                    };

                    if let Ok(mut state) = progress_state.lock() {
                        state.remove(&name);
                    }
                    last_progress_render.remove(&name);
                    message!(message_callback, "__UPGRADE_PROGRESS_DONE__ {}", name);

                    let transfer = Self::format_transfer(downloaded, bytes_total);
                    match result {
                        Ok(updated) => {
                            updated_packages.push(updated);
                            upgraded += 1;
                            message!(
                                message_callback,
                                "[ok] {:<28} upgraded to {:<13} {}",
                                name,
                                new_version,
                                transfer
                            );
                        }
                        Err(err) => {
                            failures += 1;
                            message!(
                                message_callback,
                                "[fail] {:<28} {}",
                                name,
                                Self::truncate_error(&err.to_string(), 64)
                            );
                        }
                    }

                    completed += 1;
                    if let Some(cb) = overall_progress.as_mut() {
                        cb(completed, total);
                    }
                }
                _ = ticker.tick() => {
                    Self::emit_progress_updates(&progress_state, &mut last_progress_render, message_callback);
                }
            }
        }

        message!(message_callback, "__UPGRADE_PROGRESS_CLEAR__");

        let _ = download_progress;

        for updated in updated_packages {
            self.package_storage.add_or_update_package(updated)?;
        }
        self.package_storage
            .save_packages()
            .context("Failed to save updated package information")?;

        Ok((upgraded, failures))
    }

    pub async fn upgrade_single<F, H>(
        &mut self,
        package_name: &str,
        force_option: &bool,
        trust_mode: TrustMode,
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
                trust_mode,
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
mod tests {
    use super::{ProgressState, UpgradeOperation};
    use crate::models::common::enums::{Channel, Provider};
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    #[test]
    fn truncate_error_adds_ellipsis_when_limit_exceeded() {
        let input = "this is a fairly long error string";
        let truncated = UpgradeOperation::truncate_error(input, 12);
        assert!(truncated.ends_with("..."));
        assert!(truncated.chars().count() <= 12);
    }

    #[test]
    fn format_transfer_handles_known_unknown_and_empty_sizes() {
        assert_eq!(UpgradeOperation::format_transfer(0, 0), "-");
        assert!(UpgradeOperation::format_transfer(42, 0).contains("42"));
        let known_total = UpgradeOperation::format_transfer(1024, 2048);
        assert!(known_total.contains('/'));
    }

    #[test]
    fn render_progress_row_includes_package_channel_provider_and_transfer() {
        let row = UpgradeOperation::render_progress_row(
            "ripgrep",
            &Channel::Stable,
            &Provider::Github,
            128,
            256,
        );
        assert!(row.contains("ripgrep"));
        assert!(row.contains("stable"));
        assert!(row.contains("github"));
        assert!(row.contains('/'));
    }

    #[test]
    fn download_progress_state_removes_known_completed_transfers() {
        let state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));

        UpgradeOperation::record_download_progress(
            &state,
            "ripgrep",
            &Channel::Stable,
            &Provider::Github,
            128,
            256,
        );
        assert!(state.lock().expect("state").contains_key("ripgrep"));

        UpgradeOperation::record_download_progress(
            &state,
            "ripgrep",
            &Channel::Stable,
            &Provider::Github,
            256,
            256,
        );
        assert!(!state.lock().expect("state").contains_key("ripgrep"));
    }

    #[test]
    fn progress_updates_emit_done_for_rows_removed_by_callbacks() {
        let state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let mut last_render = BTreeMap::new();
        let messages = Rc::new(RefCell::new(Vec::new()));
        let captured_messages = Rc::clone(&messages);
        let mut callback =
            Some(move |msg: &str| captured_messages.borrow_mut().push(msg.to_string()));

        UpgradeOperation::record_download_progress(
            &state,
            "ripgrep",
            &Channel::Stable,
            &Provider::Github,
            128,
            256,
        );
        UpgradeOperation::emit_progress_updates(&state, &mut last_render, &mut callback);
        assert!(
            messages
                .borrow()
                .iter()
                .any(|msg| msg.starts_with("__UPGRADE_PROGRESS_ROW__ ripgrep\t"))
        );

        UpgradeOperation::record_download_progress(
            &state,
            "ripgrep",
            &Channel::Stable,
            &Provider::Github,
            256,
            256,
        );
        UpgradeOperation::emit_progress_updates(&state, &mut last_render, &mut callback);
        assert!(
            messages
                .borrow()
                .iter()
                .any(|msg| msg == "__UPGRADE_PROGRESS_DONE__ ripgrep")
        );
    }
}
