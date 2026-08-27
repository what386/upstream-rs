use crate::{
    application::cancellation,
    models::{
        common::enums::{Channel, Provider, TrustMode},
        upstream::{Package, config::ConcurrencyConfig},
    },
    output,
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{
            PackageActivator, PackageChecker, PackageInstaller, PackageProgressEvent,
            PackageUpgrader, ResolvedUpgradeTarget, RollbackManager,
            disk_impact::{DiskImpact, SignedByteEstimate},
        },
        trust::TrustedSignatureKeys,
    },
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, anyhow};
use futures_util::stream::{self, FuturesUnordered, StreamExt};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

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

fn build_ref_version(label: impl AsRef<str>, commit: Option<&str>) -> String {
    let label = label.as_ref();

    let Some(commit) = commit else {
        return label.to_string();
    };

    let short: String = commit.chars().take(7).collect();
    format!("{label}@{short}")
}

type SharedProgressCallback<'a, P> = Arc<Mutex<&'a mut Option<P>>>;

fn emit_progress<P>(callback: &SharedProgressCallback<'_, P>, event: UpgradeProgressEvent)
where
    P: FnMut(UpgradeProgressEvent),
{
    if let Ok(mut callback) = callback.lock()
        && let Some(callback) = (**callback).as_mut()
    {
        callback(event);
    }
}

fn emit_package_progress<P>(
    callback: &SharedProgressCallback<'_, P>,
    name: &str,
    event: PackageProgressEvent,
) where
    P: FnMut(UpgradeProgressEvent),
{
    let event = match event {
        PackageProgressEvent::Warning(message) => UpgradeProgressEvent::Warning {
            name: name.to_string(),
            message,
        },
        event => UpgradeProgressEvent::Package {
            name: name.to_string(),
            event,
        },
    };

    emit_progress(callback, event);
}

pub struct UpgradeOperation<'a> {
    upgrader: PackageUpgrader<'a>,
    checker: PackageChecker<'a>,
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
    async fn check_packages(
        &self,
        packages: Vec<crate::models::upstream::Package>,
        checking_callback: &mut dyn FnMut(&str),
    ) -> Vec<UpdateCheckRow> {
        self.checker
            .check_many(packages, checking_callback)
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
        Ok(Self {
            upgrader: PackageUpgrader::new(provider_manager, installer, paths, trusted_keys),
            checker: PackageChecker::new(provider_manager, concurrency_config),
            paths,
            package_database,
            concurrency_config,
        })
    }

    pub fn estimate_upgrade_rollback_impact(
        &self,
        rows: &[UpgradePreviewRow],
    ) -> SignedByteEstimate {
        let Ok(manager) = RollbackManager::new(self.paths) else {
            return SignedByteEstimate::unknown();
        };

        rows.iter()
            .map(|row| {
                let Some(package) = self.package_database.get_package(&row.name).ok().flatten()
                else {
                    return SignedByteEstimate::unknown();
                };

                manager
                    .estimate_capture_impact(&package)
                    .unwrap_or_else(|_| SignedByteEstimate::unknown())
            })
            .fold(SignedByteEstimate::exact(0), |total, impact| total + impact)
    }

    pub async fn preview_upgrade<H>(
        &self,
        names: Option<&[String]>,
        force: bool,
        event_callback: &mut H,
    ) -> Result<Vec<UpgradePreviewRow>>
    where
        H: FnMut(UpgradePreviewEvent),
    {
        if let Some(names) = names {
            let mut unique = BTreeSet::new();
            if let Some(duplicate) = names.iter().find(|name| !unique.insert(name.as_str())) {
                return Err(anyhow!(
                    "Package '{}' was requested more than once",
                    duplicate
                ));
            }
        }

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
            let row = row?;

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
        package: Package,
        force: bool,
    ) -> (usize, Result<Option<UpgradePreviewRow>>) {
        (idx, self.preview_package_upgrade(package, force).await)
    }

    async fn preview_package_upgrade(
        &self,
        package: Package,
        force: bool,
    ) -> Result<Option<UpgradePreviewRow>> {
        let Some(plan) = self.upgrader.plan_upgrade(&package, force).await? else {
            return Ok(None);
        };

        let source_build = package.install_type == crate::models::upstream::InstallType::Build;

        let old_version = if source_build {
            build_ref_version(package.version.to_string(), package.build_commit.as_deref())
        } else {
            package.version.to_string()
        };

        let new_version = match &plan.target {
            ResolvedUpgradeTarget::Release(release) => release.version.to_string(),

            ResolvedUpgradeTarget::Branch {
                branch,
                head_commit,
            } => build_ref_version(branch, Some(head_commit)),
        };

        Ok(Some(UpgradePreviewRow {
            name: package.name.clone(),
            source: preview_package_source(&package),
            old_version,
            new_version,
            disk_impact: plan.disk_impact,
            source_build,
            target: plan.target,
            package,
        }))
    }

    pub async fn upgrade_resolved_bulk<P>(
        &mut self,
        rows: &[UpgradePreviewRow],
        trust_mode: Option<TrustMode>,
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

                let effective_trust_mode = self
                    .package_database
                    .effective_trust_mode(&row.name, trust_mode)?;

                Ok((package, row.clone(), effective_trust_mode))
            })
            .collect::<Result<Vec<_>>>()?;

        let mut completed = 0_u32;
        let mut upgraded = 0_u32;
        let mut failures = 0_u32;
        let progress_callback = Arc::new(Mutex::new(progress_callback));

        emit_progress(
            &progress_callback,
            UpgradeProgressEvent::Overall { completed, total },
        );

        let mut pending = stream::iter(packages.into_iter().map(|(package, row, trust_mode)| {
            let progress_callback = Arc::clone(&progress_callback);

            async move {
                let name = package.name.clone();
                let new_version = row.new_version.clone();
                let progress_name = name.clone();
                let package_progress_callback = Arc::clone(&progress_callback);

                let mut no_download_progress: Option<fn(u64, u64)> = None;
                let mut ignored_messages = Some(|_: &str| {});
                let mut progress_cb = Some(move |event: PackageProgressEvent| {
                    emit_package_progress(&package_progress_callback, &progress_name, event);
                });

                let result = upgrader
                    .upgrade_resolved(
                        &package,
                        row.target,
                        trust_mode,
                        &mut no_download_progress,
                        &mut ignored_messages,
                        &mut progress_cb,
                    )
                    .await
                    .context(format!("Failed to upgrade package '{}'", name));

                (name, new_version, result)
            }
        }))
        .buffer_unordered(self.concurrency_config.install_concurrency());

        // Drain every in-flight result after cancellation. A replacement may
        // already be complete and buffered here, so returning early could drop
        // its result before the matching database commit.
        let mut interrupted = false;

        while let Some((name, new_version, result)) = pending.next().await {
            interrupted |= cancellation::is_requested();

            match result {
                Ok(updated) => {
                    match complete_upgrade(
                        self.paths,
                        self.package_database,
                        &progress_callback,
                        name.clone(),
                        &updated,
                        new_version,
                    ) {
                        Ok(()) => {
                            upgraded += 1;
                        }
                        Err(err) => {
                            failures += 1;
                            emit_progress(
                                &progress_callback,
                                UpgradeProgressEvent::Complete {
                                    name,
                                    result: UpgradePackageResult::Failed {
                                        error: output::error_summary(&err),
                                    },
                                },
                            );
                        }
                    }
                }
                Err(err) => {
                    failures += 1;
                    emit_progress(
                        &progress_callback,
                        UpgradeProgressEvent::Complete {
                            name,
                            result: UpgradePackageResult::Failed {
                                error: output::error_summary(&err),
                            },
                        },
                    );
                }
            }

            completed += 1;
            emit_progress(
                &progress_callback,
                UpgradeProgressEvent::Overall { completed, total },
            );
        }

        emit_progress(&progress_callback, UpgradeProgressEvent::Clear);

        if interrupted || cancellation::is_requested() {
            cancellation::check()?;
        }

        Ok((upgraded, failures))
    }

    pub async fn check_detailed(
        &self,
        package_names: Option<&[String]>,
        checking_callback: &mut dyn FnMut(&str),
    ) -> Result<Vec<UpdateCheckRow>> {
        let Some(package_names) = package_names else {
            let packages = self.package_database.list_packages()?;
            return Ok(self.check_packages(packages, checking_callback).await);
        };

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
                Ok(None) => {
                    rows[idx] = Some(UpdateCheckRow {
                        name: name.clone(),
                        channel: None,
                        provider: None,
                        status: UpdateCheckStatus::NotInstalled,
                    })
                }
                Err(err) => {
                    return Err(err).context(format!(
                        "Failed to load package '{}' for update check",
                        name
                    ));
                }
            }
        }

        let checked_rows = self
            .check_packages(selected_packages, checking_callback)
            .await;

        for (row_idx, checked_row) in selected_indices.into_iter().zip(checked_rows) {
            rows[row_idx] = Some(checked_row);
        }

        Ok(rows.into_iter().flatten().collect())
    }
}

fn complete_upgrade<P>(
    paths: &UpstreamPaths,
    package_database: &mut PackageDatabase,
    progress_callback: &SharedProgressCallback<'_, P>,
    name: String,
    updated: &crate::models::upstream::Package,
    version: String,
) -> Result<()>
where
    P: FnMut(UpgradeProgressEvent),
{
    PackageActivator::new(paths).persist(package_database, updated)?;
    emit_progress(
        progress_callback,
        UpgradeProgressEvent::Complete {
            name,
            result: UpgradePackageResult::Upgraded { version },
        },
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        UpgradePackageResult, UpgradeProgressEvent, complete_upgrade, emit_package_progress,
        emit_progress, preview_package_width,
    };
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::services::packaging::{PackagePhase, PackageProgressEvent};
    use crate::storage::database::PackageDatabase;
    use crate::utils::test_support;
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
    fn package_progress_is_forwarded_immediately() {
        let mut events = Vec::new();
        let mut callback = Some(|event: UpgradeProgressEvent| events.push(event));
        let callback = Arc::new(Mutex::new(&mut callback));

        emit_package_progress(
            &callback,
            "ripgrep",
            PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot),
        );

        emit_package_progress(
            &callback,
            "ripgrep",
            PackageProgressEvent::Extraction {
                extracted: 128,
                total: 256,
            },
        );

        drop(callback);

        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            UpgradeProgressEvent::Package {
                name,
                event: PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot),
            } if name == "ripgrep"
        ));

        assert!(matches!(
            &events[1],
            UpgradeProgressEvent::Package {
                name,
                event: PackageProgressEvent::Extraction {
                    extracted: 128,
                    total: 256,
                },
            } if name == "ripgrep"
        ));
    }

    #[test]
    fn warning_progress_is_forwarded_as_warning_event() {
        let mut events = Vec::new();
        let mut callback = Some(|event: UpgradeProgressEvent| events.push(event));
        let callback = Arc::new(Mutex::new(&mut callback));

        emit_package_progress(
            &callback,
            "ripgrep",
            PackageProgressEvent::Warning("fallback used".to_string()),
        );

        drop(callback);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            UpgradeProgressEvent::Warning { name, message }
                if name == "ripgrep" && message == "fallback used"
        ));
    }

    #[test]
    fn overall_progress_is_emitted_directly() {
        let mut events = Vec::new();
        let mut callback = Some(|event: UpgradeProgressEvent| events.push(event));
        let callback = Arc::new(Mutex::new(&mut callback));

        emit_progress(
            &callback,
            UpgradeProgressEvent::Overall {
                completed: 1,
                total: 3,
            },
        );

        drop(callback);

        assert_eq!(
            events,
            vec![UpgradeProgressEvent::Overall {
                completed: 1,
                total: 3,
            }]
        );
    }

    #[test]
    fn upgrade_completion_callback_observes_persisted_package_state() {
        let root = test_support::temp_root("upstream-upgrade-op-test", "completion-order");
        let paths = test_support::upstream_paths(&root);
        let path = paths.metadata.packages_database_file.clone();

        let mut database = PackageDatabase::open(&path).expect("open database");
        let mut stored = test_package("tool", Channel::Stable);
        stored.version = crate::models::common::Version::new(1, 0, 0, false);
        database.upsert_package(&stored).expect("seed package");

        let mut updated = stored.clone();
        updated.version = crate::models::common::Version::new(2, 0, 0, false);
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

                    assert_eq!(
                        package.version,
                        crate::models::common::Version::new(2, 0, 0, false)
                    );
                }
            });

            let callback = Arc::new(Mutex::new(&mut callback));

            complete_upgrade(
                &paths,
                &mut database,
                &callback,
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
