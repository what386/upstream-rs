use crate::{
    application::cancellation,
    models::common::enums::{Channel, Provider, TrustMode},
    models::provider::Release,
    models::upstream::config::ConcurrencyConfig,
    output::{self, Status},
    providers::provider_manager::ProviderManager,
    services::packaging::disk_impact::{
        ByteEstimate, DiskImpact, SignedByteEstimate, asset_size_estimate, estimate_path_size,
    },
    services::{
        integration::ShellManager,
        packaging::{
            PackageChecker, PackageInstaller, PackageProgressEvent, PackageRemover,
            PackageUpgrader, ResolvedUpgradeTarget,
        },
        trust::TrustedSignatureKeys,
    },
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, anyhow};
use futures_util::stream::{self, FuturesUnordered, StreamExt};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration};

#[derive(Clone)]
struct ProgressEntry {
    event: PackageProgressEvent,
}
type ProgressState = Arc<Mutex<BTreeMap<String, ProgressEntry>>>;
type WarningState = Arc<Mutex<Vec<(String, String)>>>;

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

fn preview_package_source(package: &crate::models::upstream::Package) -> String {
    package.channel.to_string().to_lowercase()
}

fn preview_package_label(package: &crate::models::upstream::Package) -> String {
    format!("{}/{}", preview_package_source(package), package.name)
}

fn preview_package_width(packages: &[crate::models::upstream::Package]) -> usize {
    packages
        .iter()
        .map(|package| preview_package_label(package).chars().count())
        .chain(std::iter::once("Package".len()))
        .max()
        .unwrap_or("Package".len())
}

pub struct UpgradeOperation<'a> {
    upgrader: PackageUpgrader<'a>,
    checker: PackageChecker<'a>,
    provider_manager: &'a ProviderManager,
    paths: &'a UpstreamPaths,
    package_database: &'a mut PackageDatabase,
    concurrency_config: ConcurrencyConfig,
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
    pub package: crate::models::upstream::Package,
    pub name: String,
    pub source: String,
    pub old_version: String,
    pub new_version: String,
    pub disk_impact: DiskImpact,
    pub source_build: bool,
    pub target: ResolvedUpgradeTarget,
}

pub enum UpgradePreviewEvent {
    Started { package_width: usize },
    Checking { name: String },
    Row(Box<UpgradePreviewRow>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradePackageResult {
    Upgraded { version: String },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeProgressEvent {
    Overall {
        completed: u32,
        total: u32,
    },
    Package {
        name: String,
        event: PackageProgressEvent,
    },
    Warning {
        name: String,
        message: String,
    },
    Complete {
        name: String,
        result: UpgradePackageResult,
    },
    Clear,
}

impl<'a> UpgradeOperation<'a> {
    fn record_download_progress(
        progress_state: &ProgressState,
        name: &str,
        downloaded: u64,
        total: u64,
    ) {
        let Ok(mut state) = progress_state.lock() else {
            return;
        };

        state.insert(
            name.to_string(),
            ProgressEntry {
                event: PackageProgressEvent::Download { downloaded, total },
            },
        );
    }

    fn record_zsync_progress(
        progress_state: &ProgressState,
        name: &str,
        downloaded: u64,
        total: u64,
    ) {
        let Ok(mut state) = progress_state.lock() else {
            return;
        };

        state.insert(
            name.to_string(),
            ProgressEntry {
                event: PackageProgressEvent::Zsync { downloaded, total },
            },
        );
    }

    fn record_checksum_progress(
        progress_state: &ProgressState,
        name: &str,
        checked: u64,
        total: u64,
    ) {
        let Ok(mut state) = progress_state.lock() else {
            return;
        };

        state.insert(
            name.to_string(),
            ProgressEntry {
                event: PackageProgressEvent::Checksum { checked, total },
            },
        );
    }

    fn record_status_progress(
        progress_state: &ProgressState,
        name: &str,
        event: PackageProgressEvent,
    ) {
        let Ok(mut state) = progress_state.lock() else {
            return;
        };

        state.insert(name.to_string(), ProgressEntry { event });
    }

    fn record_progress_event(
        progress_state: &ProgressState,
        warning_state: &WarningState,
        name: &str,
        event: PackageProgressEvent,
    ) {
        match event {
            PackageProgressEvent::Phase(phase) => Self::record_status_progress(
                progress_state,
                name,
                PackageProgressEvent::Phase(phase),
            ),
            PackageProgressEvent::Download { downloaded, total } => {
                Self::record_download_progress(progress_state, name, downloaded, total)
            }
            PackageProgressEvent::Zsync { downloaded, total } => {
                Self::record_zsync_progress(progress_state, name, downloaded, total)
            }
            PackageProgressEvent::Checksum { checked, total } => {
                Self::record_checksum_progress(progress_state, name, checked, total)
            }
            PackageProgressEvent::Warning(message) => {
                if let Ok(mut warnings) = warning_state.lock() {
                    warnings.push((name.to_string(), message));
                }
            }
        }
    }

    fn emit_progress_updates<P>(
        progress_state: &ProgressState,
        warning_state: &WarningState,
        last_progress_events: &mut BTreeMap<String, PackageProgressEvent>,
        progress_callback: &mut Option<P>,
    ) where
        P: FnMut(UpgradeProgressEvent),
    {
        let warnings = warning_state
            .lock()
            .map(|mut warnings| warnings.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(cb) = progress_callback.as_mut() {
            for (name, message) in warnings {
                cb(UpgradeProgressEvent::Warning { name, message });
            }
        }

        let snapshot = progress_state
            .lock()
            .map(|state| {
                state
                    .iter()
                    .map(|(name, entry)| (name.clone(), entry.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();

        for (name, entry) in &snapshot {
            let changed = last_progress_events
                .get(name)
                .map(|prev| prev != &entry.event)
                .unwrap_or(true);
            if changed {
                if let Some(cb) = progress_callback.as_mut() {
                    cb(UpgradeProgressEvent::Package {
                        name: name.clone(),
                        event: entry.event.clone(),
                    });
                }
                last_progress_events.insert(name.clone(), entry.event.clone());
            }
        }

        let stale_names = last_progress_events
            .keys()
            .filter(|name| !snapshot.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in stale_names {
            last_progress_events.remove(&name);
        }
    }

    fn clear_completed_progress<P>(
        progress_state: &ProgressState,
        warning_state: &WarningState,
        last_progress_events: &mut BTreeMap<String, PackageProgressEvent>,
        name: &str,
        progress_callback: &mut Option<P>,
    ) where
        P: FnMut(UpgradeProgressEvent),
    {
        Self::emit_progress_updates(
            progress_state,
            warning_state,
            last_progress_events,
            progress_callback,
        );

        if let Ok(mut state) = progress_state.lock() {
            state.remove(name);
        }
        last_progress_events.remove(name);
    }

    async fn check_packages_parallel(
        &self,
        packages: Vec<crate::models::upstream::Package>,
        checking_callback: &mut dyn FnMut(&str),
    ) -> Vec<(
        crate::models::upstream::Package,
        Result<Option<(String, String)>>,
    )> {
        let package_count = packages.len();
        let mut checked = Vec::with_capacity(package_count);
        let mut package_iter = packages.into_iter().enumerate();
        let mut pending = FuturesUnordered::new();

        for _ in 0..self.concurrency_config.check_concurrency() {
            let Some((idx, pkg)) = package_iter.next() else {
                break;
            };
            checking_callback(&pkg.name);
            pending.push(self.check_package_at_index(idx, pkg));
        }

        while let Some((idx, pkg, result)) = pending.next().await {
            checked.push((idx, pkg, result));

            if let Some((next_idx, next_pkg)) = package_iter.next() {
                checking_callback(&next_pkg.name);
                pending.push(self.check_package_at_index(next_idx, next_pkg));
            }
        }

        checked.sort_by_key(|(idx, _, _)| *idx);
        checked
            .into_iter()
            .map(|(_, pkg, result)| (pkg, result))
            .collect()
    }

    async fn check_package_at_index(
        &self,
        idx: usize,
        pkg: crate::models::upstream::Package,
    ) -> (
        usize,
        crate::models::upstream::Package,
        Result<Option<(String, String)>>,
    ) {
        let result = self.checker.check_one(&pkg).await;
        (idx, pkg, result)
    }

    async fn check_installed_packages_detailed_with_callback(
        &self,
        packages: Vec<crate::models::upstream::Package>,
        checking_callback: &mut dyn FnMut(&str),
    ) -> Vec<UpdateCheckRow> {
        self.check_packages_parallel(packages, checking_callback)
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
        package_database: &'a mut PackageDatabase,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
        concurrency_config: ConcurrencyConfig,
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
            package_database,
            concurrency_config,
        })
    }

    pub fn estimate_upgrade_rollback_impact(
        &self,
        rows: &[UpgradePreviewRow],
    ) -> SignedByteEstimate {
        rows.iter()
            .map(|row| {
                let Some(package) = self.package_database.get_package(&row.name).ok().flatten()
                else {
                    return SignedByteEstimate::unknown();
                };
                let active_size = PackageRemover::new(self.paths)
                    .estimate_active_size(&package)
                    .unwrap_or(0);
                let existing_rollback =
                    estimate_path_size(&self.paths.state.rollback_dir.join(&package.name))
                        .unwrap_or(0);
                SignedByteEstimate::exact(
                    i128::from(active_size).saturating_sub(i128::from(existing_rollback)),
                )
            })
            .fold(SignedByteEstimate::exact(0), |total, impact| total + impact)
    }

    pub async fn estimate_upgrade_impact(
        &self,
        names: Option<&[String]>,
        force: bool,
    ) -> DiskImpact {
        let mut ignored_messages = Some(|_: &str| {});
        let Ok(rows) = self
            .preview_upgrade_with_messages(names, force, &mut ignored_messages)
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
        let mut ignored_messages = Some(|_: &str| {});
        self.preview_upgrade_with_messages(names, force, &mut ignored_messages)
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
                    self.package_database
                        .get_package(name)?
                        .ok_or_else(|| anyhow!("Package '{}' is not installed", name))
                })
                .collect::<Result<Vec<_>>>()?,
            None => self.package_database.list_packages()?,
        };
        let package_width = preview_package_width(&packages);
        event_callback(UpgradePreviewEvent::Started { package_width });

        let package_count = packages.len();
        let mut rows_by_index: Vec<Option<UpgradePreviewRow>> =
            (0..package_count).map(|_| None).collect();
        let mut package_iter = packages.into_iter().enumerate();
        let mut pending = FuturesUnordered::new();

        for _ in 0..self.concurrency_config.check_concurrency() {
            let Some((idx, package)) = package_iter.next() else {
                break;
            };
            event_callback(UpgradePreviewEvent::Checking {
                name: package.name.clone(),
            });
            pending.push(self.preview_package_at_index(idx, package, force));
        }

        while let Some((idx, row)) = pending.next().await {
            if let Some(row) = row.clone() {
                event_callback(UpgradePreviewEvent::Row(Box::new(row)));
            }
            rows_by_index[idx] = row;

            if let Some((next_idx, next_package)) = package_iter.next() {
                event_callback(UpgradePreviewEvent::Checking {
                    name: next_package.name.clone(),
                });
                pending.push(self.preview_package_at_index(next_idx, next_package, force));
            }
        }

        Ok(rows_by_index.into_iter().flatten().collect())
    }

    async fn preview_package_at_index(
        &self,
        idx: usize,
        package: crate::models::upstream::Package,
        force: bool,
    ) -> (usize, Option<UpgradePreviewRow>) {
        (idx, self.preview_package_upgrade(package, force).await)
    }

    async fn preview_package_upgrade(
        &self,
        package: crate::models::upstream::Package,
        force: bool,
    ) -> Option<UpgradePreviewRow> {
        if package.is_pinned {
            return None;
        }

        if package.install_type == crate::models::upstream::InstallType::Build
            && let Some(branch) = package.build_branch.as_deref()
        {
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
                package: package.clone(),
                name: package.name.clone(),
                source: preview_package_source(&package),
                old_version: build_ref_version(
                    package.version.to_string(),
                    package.build_commit.as_deref(),
                ),
                new_version: build_ref_version(branch, Some(&head_commit)),
                disk_impact: DiskImpact::unknown(),
                source_build: true,
                target: ResolvedUpgradeTarget::Branch {
                    branch: branch.to_string(),
                    head_commit,
                },
            });
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

        if !force && !package.is_update_available(&release) {
            return None;
        }

        let source_build = package.install_type == crate::models::upstream::InstallType::Build;
        Some(UpgradePreviewRow {
            package: package.clone(),
            name: package.name.clone(),
            source: preview_package_source(&package),
            old_version: package.version.to_string(),
            new_version: release.version.to_string(),
            disk_impact: if source_build {
                DiskImpact::unknown()
            } else {
                self.estimate_release_upgrade_impact(&package, &release)
            },
            source_build,
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
        let active_size = PackageRemover::new(self.paths)
            .estimate_active_size(package)
            .unwrap_or(0);
        match new_size.bytes {
            Some(bytes) => DiskImpact {
                download: new_size,
                net: SignedByteEstimate::estimated(
                    i128::from(bytes).saturating_sub(i128::from(active_size)),
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
            .package_database
            .list_packages()?
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
        cancellation::check()?;
        let total = names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;
        let mut upgraded = 0;
        let force = *force_option;
        let upgrader = &self.upgrader;
        let completion_subject_width =
            output::status_subject_width(names.iter().map(String::as_str));

        let packages: Vec<_> = names
            .iter()
            .map(|name| {
                self.package_database
                    .get_package(name)?
                    .ok_or_else(|| anyhow!("Package '{}' is not installed", name))
            })
            .collect::<Result<Vec<_>>>()?;

        let mut pending = stream::iter(packages.into_iter().map(|package| async move {
            let name = package.name.clone();
            let channel = package.channel.clone();
            let provider = package.provider.clone();

            let mut downloaded: u64 = 0;
            let mut bytes_total: u64 = 0;
            let mut download_cb = Some(|d: u64, t: u64| {
                downloaded = d;
                bytes_total = t;
            });
            let mut ignored_messages = Some(|_: &str| {});

            let result = upgrader
                .upgrade(
                    &package,
                    force,
                    trust_mode,
                    &mut download_cb,
                    &mut ignored_messages,
                )
                .await
                .context(format!("Failed to upgrade package '{}'", name));
            (name, channel, provider, downloaded, bytes_total, result)
        }))
        .buffer_unordered(self.concurrency_config.install_concurrency());

        while completed < total {
            cancellation::check()?;
            let Some((name, channel, provider, downloaded, bytes_total, result)) =
                pending.next().await
            else {
                break;
            };

            let transfer = format_transfer(downloaded, bytes_total);
            match result {
                Ok(Some(updated)) => {
                    let persist_result = self.package_database.upsert_package(&updated);
                    let persist_result = persist_result
                        .and_then(|()| refresh_shell_paths(self.paths, self.package_database));
                    if let Err(err) = persist_result {
                        message!(
                            message_callback,
                            "{}",
                            output::status_line_text_with_width(
                                Status::Fail,
                                &name,
                                format!(
                                    "{:<10} {:<3} {:<10} {}",
                                    channel.to_string().to_lowercase(),
                                    "!",
                                    provider.to_string(),
                                    output::error_summary_with_limit(&err, 96)
                                ),
                                completion_subject_width
                            )
                        );
                        failures += 1;
                    } else {
                        message!(
                            message_callback,
                            "{}",
                            output::status_line_text_with_width(
                                Status::Ok,
                                &name,
                                format!(
                                    "{:<10} {:<3} {:<10} {}",
                                    channel.to_string().to_lowercase(),
                                    "u",
                                    provider.to_string(),
                                    transfer
                                ),
                                completion_subject_width
                            )
                        );
                        upgraded += 1;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    message!(
                        message_callback,
                        "{}",
                        output::status_line_text_with_width(
                            Status::Fail,
                            &name,
                            format!(
                                "{:<10} {:<3} {:<10} {}",
                                channel.to_string().to_lowercase(),
                                "!",
                                provider.to_string(),
                                output::error_summary_with_limit(&e, 96)
                            ),
                            completion_subject_width
                        )
                    );
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress.as_mut() {
                cb(completed, total);
            }
        }

        // Bulk mode uses per-package workers; a single shared download progress bar is noisy.
        let _ = download_progress;

        message!(
            message_callback,
            "Completed: {} upgraded, {} up-to-date, {} failed",
            upgraded,
            total - upgraded - failures,
            failures
        );

        Ok(())
    }

    pub async fn upgrade_resolved_bulk<P>(
        &mut self,
        rows: &[UpgradePreviewRow],
        trust_mode: TrustMode,
        progress_callback: &mut Option<P>,
    ) -> Result<(u32, u32)>
    where
        P: FnMut(UpgradeProgressEvent),
    {
        cancellation::check()?;
        let total = rows.len() as u32;
        let upgrader = &self.upgrader;
        let packages = rows
            .iter()
            .map(|row| {
                let package = self
                    .package_database
                    .get_package(&row.name)?
                    .ok_or_else(|| anyhow!("Package '{}' is not installed", row.name))?
                    .clone();
                Ok((package, row.clone()))
            })
            .collect::<Result<Vec<_>>>()?;

        let mut completed = 0;
        let mut upgraded = 0;
        let mut failures = 0;
        let progress_state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let warning_state: WarningState = Arc::new(Mutex::new(Vec::new()));
        let mut last_progress_events: BTreeMap<String, PackageProgressEvent> = BTreeMap::new();
        if let Some(cb) = progress_callback.as_mut() {
            cb(UpgradeProgressEvent::Overall { completed, total });
        }
        let mut pending = stream::iter(packages.into_iter().map(|(package, row)| {
            let state_ref = Arc::clone(&progress_state);
            let warning_state_ref = Arc::clone(&warning_state);
            async move {
                let name = package.name.clone();
                let new_version = row.new_version.clone();

                let mut downloaded: u64 = 0;
                let mut bytes_total: u64 = 0;
                let mut download_cb = Some(|d: u64, t: u64| {
                    downloaded = d;
                    bytes_total = t;
                    Self::record_download_progress(&state_ref, &name, d, t);
                });
                let mut ignored_messages = Some(|_: &str| {});
                let mut progress_cb = Some(|event: PackageProgressEvent| {
                    Self::record_progress_event(&state_ref, &warning_state_ref, &name, event);
                });

                let result = upgrader
                    .upgrade_resolved(
                        &package,
                        row.target,
                        trust_mode,
                        &mut download_cb,
                        &mut ignored_messages,
                        &mut progress_cb,
                    )
                    .await
                    .context(format!("Failed to upgrade package '{}'", name));

                (name, new_version, downloaded, bytes_total, result)
            }
        }))
        .buffer_unordered(self.concurrency_config.install_concurrency());

        let mut ticker = time::interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        while completed < total {
            cancellation::check()?;
            tokio::select! {
                maybe_item = pending.next() => {
                    let Some((name, new_version, _downloaded, _bytes_total, result)) = maybe_item else {
                        break;
                    };

                    Self::clear_completed_progress(
                        &progress_state,
                        &warning_state,
                        &mut last_progress_events,
                        &name,
                        progress_callback,
                    );

                    match result {
                        Ok(updated) => {
                            match persist_upgrade_and_emit_complete(
                                self.paths,
                                self.package_database,
                                progress_callback,
                                name.clone(),
                                &updated,
                                new_version,
                            ) {
                                Ok(()) => {
                                    upgraded += 1;
                                }
                                Err(err) => {
                                    failures += 1;
                                    if let Some(cb) = progress_callback.as_mut() {
                                        cb(UpgradeProgressEvent::Complete {
                                            name,
                                            result: UpgradePackageResult::Failed {
                                                error: output::error_summary(&err),
                                            },
                                        });
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            failures += 1;
                            if let Some(cb) = progress_callback.as_mut() {
                                cb(UpgradeProgressEvent::Complete {
                                    name,
                                    result: UpgradePackageResult::Failed {
                                        error: output::error_summary(&err),
                                    },
                                });
                            }
                        }
                    }

                    completed += 1;
                    if let Some(cb) = progress_callback.as_mut() {
                        cb(UpgradeProgressEvent::Overall {
                            completed,
                            total,
                        });
                    }
                }
                _ = ticker.tick() => {
                    Self::emit_progress_updates(&progress_state, &warning_state, &mut last_progress_events, progress_callback);
                }
            }
        }

        if let Some(cb) = progress_callback.as_mut() {
            cb(UpgradeProgressEvent::Clear);
        }

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
        cancellation::check()?;
        let package = self
            .package_database
            .get_package(package_name)?
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

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
            self.package_database.upsert_package(&updated)?;
            refresh_shell_paths(self.paths, self.package_database)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn check_all_detailed(&self) -> Vec<UpdateCheckRow> {
        let mut ignored_callback = |_: &str| {};
        self.check_all_detailed_with_callback(&mut ignored_callback)
            .await
    }

    pub async fn check_all_detailed_with_callback(
        &self,
        checking_callback: &mut dyn FnMut(&str),
    ) -> Vec<UpdateCheckRow> {
        let packages = self.package_database.list_packages().unwrap_or_default();
        self.check_installed_packages_detailed_with_callback(packages, checking_callback)
            .await
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
        let mut ignored_callback = |_: &str| {};
        self.check_selected_detailed_with_callback(package_names, &mut ignored_callback)
            .await
    }

    pub async fn check_selected_detailed_with_callback(
        &self,
        package_names: &[String],
        checking_callback: &mut dyn FnMut(&str),
    ) -> Vec<UpdateCheckRow> {
        let mut rows: Vec<Option<UpdateCheckRow>> =
            (0..package_names.len()).map(|_| None).collect();
        let mut selected_packages = Vec::new();
        let mut selected_indices = Vec::new();

        for (idx, name) in package_names.iter().enumerate() {
            match self.package_database.get_package(name) {
                Ok(Some(package)) => {
                    selected_packages.push(package);
                    selected_indices.push(idx);
                }
                Ok(None) | Err(_) => {
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
            .check_installed_packages_detailed_with_callback(selected_packages, checking_callback)
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

fn format_transfer(downloaded: u64, total: u64) -> String {
    if total > 0 {
        format!(
            "{} / {}",
            indicatif::HumanBytes(downloaded),
            indicatif::HumanBytes(total)
        )
    } else if downloaded > 0 {
        format!("{}", indicatif::HumanBytes(downloaded))
    } else {
        "-".to_string()
    }
}

fn refresh_shell_paths(
    paths: &UpstreamPaths,
    package_database: &mut PackageDatabase,
) -> Result<()> {
    ShellManager::new(&paths.config.paths_file).regenerate_paths(package_database, paths)
}

fn persist_upgrade_and_emit_complete<P>(
    paths: &UpstreamPaths,
    package_database: &mut PackageDatabase,
    progress_callback: &mut Option<P>,
    name: String,
    updated: &crate::models::upstream::Package,
    version: String,
) -> Result<()>
where
    P: FnMut(UpgradeProgressEvent),
{
    package_database.upsert_package(updated)?;
    refresh_shell_paths(paths, package_database)?;
    if let Some(cb) = progress_callback.as_mut() {
        cb(UpgradeProgressEvent::Complete {
            name,
            result: UpgradePackageResult::Upgraded { version },
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ProgressState, UpgradeOperation, UpgradePackageResult, UpgradeProgressEvent,
        format_transfer, persist_upgrade_and_emit_complete, preview_package_width,
    };
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::services::packaging::{PackagePhase, PackageProgressEvent};
    use crate::storage::database::PackageDatabase;
    use crate::utils::test_support;
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::{Arc, Mutex};

    fn test_package(name: &str, channel: Channel) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            channel,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn preview_package_width_uses_source_prefixed_package_labels() {
        let packages = vec![
            test_package("gh", Channel::Stable),
            test_package("longer-package", Channel::Nightly),
        ];

        assert_eq!(
            preview_package_width(&packages),
            "nightly/longer-package".len()
        );
    }

    #[test]
    fn format_transfer_handles_known_unknown_and_empty_sizes() {
        assert_eq!(format_transfer(0, 0), "-");
        assert!(format_transfer(42, 0).contains("42"));
        let known_total = format_transfer(1024, 2048);
        assert!(known_total.contains('/'));
    }

    #[test]
    fn progress_state_tracks_latest_package_progress_event() {
        let state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let warnings = Arc::new(Mutex::new(Vec::new()));

        UpgradeOperation::record_progress_event(
            &state,
            &warnings,
            "ripgrep",
            PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot),
        );
        assert_eq!(
            state.lock().expect("state")["ripgrep"].event,
            PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot)
        );

        UpgradeOperation::record_download_progress(&state, "ripgrep", 128, 256);
        assert_eq!(
            state.lock().expect("state")["ripgrep"].event,
            PackageProgressEvent::Download {
                downloaded: 128,
                total: 256,
            }
        );

        UpgradeOperation::record_progress_event(
            &state,
            &warnings,
            "ripgrep",
            PackageProgressEvent::Zsync {
                downloaded: 192,
                total: 256,
            },
        );
        assert_eq!(
            state.lock().expect("state")["ripgrep"].event,
            PackageProgressEvent::Zsync {
                downloaded: 192,
                total: 256,
            }
        );

        UpgradeOperation::record_progress_event(
            &state,
            &warnings,
            "ripgrep",
            PackageProgressEvent::Phase(PackagePhase::InstallingPackage),
        );
        assert_eq!(
            state.lock().expect("state")["ripgrep"].event,
            PackageProgressEvent::Phase(PackagePhase::InstallingPackage)
        );
    }

    #[test]
    fn progress_updates_emit_typed_package_events_without_sentinels() {
        let state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let mut last_render = BTreeMap::new();
        let mut events = Vec::new();

        UpgradeOperation::record_download_progress(&state, "ripgrep", 128, 256);
        {
            let mut callback = Some(|event: UpgradeProgressEvent| events.push(event));
            UpgradeOperation::emit_progress_updates(
                &state,
                &warnings,
                &mut last_render,
                &mut callback,
            );
        }
        assert!(events.iter().any(|event| matches!(
            event,
            UpgradeProgressEvent::Package {
                name,
                event: PackageProgressEvent::Download {
                    downloaded: 128,
                    total: 256,
                },
            } if name == "ripgrep"
        )));

        {
            let mut callback = Some(|event: UpgradeProgressEvent| events.push(event));
            UpgradeOperation::emit_progress_updates(
                &state,
                &warnings,
                &mut last_render,
                &mut callback,
            );
        }
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn completed_progress_flushes_latest_package_event_before_clearing() {
        let state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let mut last_render = BTreeMap::new();
        let mut events = Vec::new();

        UpgradeOperation::record_progress_event(
            &state,
            &warnings,
            "ripgrep",
            PackageProgressEvent::Phase(PackagePhase::RollingBack),
        );

        {
            let mut callback = Some(|event: UpgradeProgressEvent| events.push(event));
            UpgradeOperation::clear_completed_progress(
                &state,
                &warnings,
                &mut last_render,
                "ripgrep",
                &mut callback,
            );
        }

        assert!(events.iter().any(|event| matches!(
            event,
            UpgradeProgressEvent::Package {
                name,
                event: PackageProgressEvent::Phase(PackagePhase::RollingBack),
            } if name == "ripgrep"
        )));
        assert!(state.lock().expect("state").is_empty());
        assert!(!last_render.contains_key("ripgrep"));
    }

    #[test]
    fn upgrade_completion_callback_observes_persisted_package_state() {
        let root = test_support::temp_root("upstream-upgrade-op-test", "completion-order");
        let paths = test_support::upstream_paths(&root);
        let path = paths.config.packages_database_file.clone();

        let mut database = PackageDatabase::open(&path).expect("open database");
        let mut stored = test_package("tool", Channel::Stable);
        stored.version.major = 1;
        database.upsert_package(&stored).expect("seed package");

        let mut updated = stored.clone();
        updated.version.major = 2;
        let updated_version = updated.version.to_string();
        let mut callback_state = Vec::new();

        {
            let mut callback = Some(|event: UpgradeProgressEvent| {
                if let UpgradeProgressEvent::Complete { name, result } = event {
                    callback_state.push((name, result));
                    let reader = PackageDatabase::open(&path).expect("open reader");
                    let package = reader
                        .get_package("tool")
                        .expect("read package in callback")
                        .expect("updated package");
                    assert_eq!(package.version.major, 2);
                }
            });

            persist_upgrade_and_emit_complete(
                &paths,
                &mut database,
                &mut callback,
                "tool".to_string(),
                &updated,
                updated_version.clone(),
            )
            .expect("persist and emit completion");
        }

        assert_eq!(callback_state.len(), 1);
        assert!(matches!(
            &callback_state[0].1,
            UpgradePackageResult::Upgraded { version } if version == &updated_version
        ));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
