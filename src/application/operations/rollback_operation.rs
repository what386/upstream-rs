use anyhow::{Result, anyhow};

use crate::{
    output,
    services::packaging::{
        RollbackManager,
        disk_impact::{ByteEstimate, DiskImpact, SignedByteEstimate},
    },
    storage::{
        package_storage::PackageStorage,
        rollback_storage::{RollbackSource, RollbackStorage},
    },
    utils::static_paths::UpstreamPaths,
};

pub struct RollbackOperation {
    paths: UpstreamPaths,
    package_storage: PackageStorage,
    rollback_storage: RollbackStorage,
}

pub struct RollbackPreviewRow {
    pub package: String,
    pub version: String,
    pub net_change: SignedByteEstimate,
}

pub struct RollbackRestoreTarget {
    pub name: String,
    pub install_path: String,
    pub source: RollbackSource,
}

pub struct RollbackPreview {
    pub rows: Vec<RollbackPreviewRow>,
    pub impact: DiskImpact,
    pub missing_names: Vec<String>,
}

pub struct RollbackRestorePreview {
    pub preview: RollbackPreview,
    pub targets: Vec<RollbackRestoreTarget>,
}

pub struct RollbackPrunePreview {
    pub target_names: Vec<String>,
    pub preview: RollbackPreview,
}

pub struct RollbackListRow {
    pub name: String,
    pub version: String,
    pub source: RollbackSource,
    pub install_path: String,
}

pub enum RollbackPackageStatus {
    Succeeded,
    Failed { error: String },
    Skipped { reason: String },
}

pub struct RollbackPackageOutcome {
    pub name: String,
    pub status: RollbackPackageStatus,
}

pub struct RollbackRestoreOutcome {
    pub restored: u32,
    pub failed: u32,
    pub packages: Vec<RollbackPackageOutcome>,
}

pub struct RollbackPruneOutcome {
    pub pruned: u32,
    pub missing: u32,
    pub packages: Vec<RollbackPackageOutcome>,
}

impl RollbackOperation {
    pub fn new() -> Result<Self> {
        let paths = UpstreamPaths::new()?;
        let package_storage = PackageStorage::new(&paths.config.packages_file)?;
        let rollback_file = RollbackManager::rollback_file_path(&paths);
        let rollback_storage = RollbackStorage::new(&rollback_file)?;

        Ok(Self {
            paths,
            package_storage,
            rollback_storage,
        })
    }

    fn manager(&mut self) -> RollbackManager<'_> {
        RollbackManager::new(
            &self.paths,
            &mut self.package_storage,
            &mut self.rollback_storage,
        )
    }

    pub fn restore_preview(&mut self, names: &[String]) -> Result<RollbackRestorePreview> {
        if names.is_empty() {
            return Err(anyhow!(
                "At least one package name is required unless --prune is provided"
            ));
        }

        let manager = self.manager();
        let preview = restore_preview(names, &manager);
        let targets = names
            .iter()
            .filter_map(|name| {
                let record = manager.rollback_record(name)?;
                Some(RollbackRestoreTarget {
                    name: name.clone(),
                    install_path: record
                        .package_snapshot
                        .install_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "<missing>".to_string()),
                    source: record.source.clone(),
                })
            })
            .collect();

        Ok(RollbackRestorePreview { preview, targets })
    }

    pub fn list_rows(&mut self) -> Vec<RollbackListRow> {
        let manager = self.manager();
        manager
            .rollback_packages()
            .into_iter()
            .filter_map(|name| {
                let record = manager.rollback_record(&name)?;
                let package = &record.package_snapshot;
                Some(RollbackListRow {
                    name,
                    version: package.version.to_string(),
                    source: record.source.clone(),
                    install_path: package
                        .install_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                })
            })
            .collect()
    }

    pub fn restorable_names(&mut self, names: &[String]) -> Vec<String> {
        let manager = self.manager();
        names
            .iter()
            .filter(|name| manager.rollback_record(name).is_some())
            .cloned()
            .collect()
    }

    pub fn restore<H>(
        &mut self,
        names: &[String],
        message_callback: &mut Option<H>,
    ) -> Result<RollbackRestoreOutcome>
    where
        H: FnMut(&str, &str),
    {
        let restorable_names = self.restorable_names(names);

        let mut restored = 0_u32;
        let mut failed = 0_u32;
        let mut packages = Vec::new();
        {
            let mut manager = self.manager();
            for name in &restorable_names {
                let package_name = name.clone();
                let mut msg = Some(|line: &str| {
                    if let Some(callback) = message_callback.as_mut() {
                        callback(&package_name, line);
                    }
                });

                match manager.restore_package(name, &mut msg) {
                    Ok(_) => {
                        packages.push(RollbackPackageOutcome {
                            name: name.clone(),
                            status: RollbackPackageStatus::Succeeded,
                        });
                        restored += 1;
                    }
                    Err(err) => {
                        let summary = output::error_summary(&err);
                        packages.push(RollbackPackageOutcome {
                            name: name.clone(),
                            status: RollbackPackageStatus::Failed {
                                error: summary.clone(),
                            },
                        });
                        failed += 1;
                    }
                }
            }
        }

        Ok(RollbackRestoreOutcome {
            restored,
            failed,
            packages,
        })
    }

    pub fn prune_preview(&mut self, names: Vec<String>) -> RollbackPrunePreview {
        let manager = self.manager();
        let target_names = if names.is_empty() {
            manager.rollback_packages()
        } else {
            names
        };
        let preview = prune_preview(&target_names, &manager);

        RollbackPrunePreview {
            target_names,
            preview,
        }
    }

    pub fn prune<H>(
        &mut self,
        target_names: &[String],
        message_callback: &mut Option<H>,
    ) -> Result<RollbackPruneOutcome>
    where
        H: FnMut(&str, usize, usize),
    {
        let mut pruned = 0_u32;
        let mut missing = 0_u32;
        let mut packages = Vec::new();
        let total = target_names.len();
        {
            let mut manager = self.manager();
            for (idx, name) in target_names.iter().enumerate() {
                if let Some(callback) = message_callback.as_mut() {
                    callback(name, idx + 1, total);
                }

                match manager.prune_package(name) {
                    Ok(true) => {
                        pruned += 1;
                        packages.push(RollbackPackageOutcome {
                            name: name.clone(),
                            status: RollbackPackageStatus::Succeeded,
                        });
                    }
                    Ok(false) => {
                        missing += 1;
                        let reason = "no rollback data found".to_string();
                        packages.push(RollbackPackageOutcome {
                            name: name.clone(),
                            status: RollbackPackageStatus::Skipped {
                                reason: reason.clone(),
                            },
                        });
                    }
                    Err(err) => {
                        let summary = output::error_summary(&err);
                        packages.push(RollbackPackageOutcome {
                            name: name.clone(),
                            status: RollbackPackageStatus::Failed {
                                error: summary.clone(),
                            },
                        });
                        return Err(err);
                    }
                }
            }
        }

        Ok(RollbackPruneOutcome {
            pruned,
            missing,
            packages,
        })
    }
}

fn restore_preview(names: &[String], manager: &RollbackManager<'_>) -> RollbackPreview {
    let rows = names
        .iter()
        .filter_map(|name| {
            let record = manager.rollback_record(name)?;
            let pkg = &record.package_snapshot;
            Some(RollbackPreviewRow {
                package: format!("{}/{}", pkg.provider, pkg.name),
                version: pkg.version.to_string(),
                net_change: manager
                    .estimate_restore_impact(name)
                    .map(|impact| impact.net)
                    .unwrap_or(SignedByteEstimate::exact(0)),
            })
        })
        .collect::<Vec<_>>();
    let missing_names = missing_names(names, &rows);
    let impact = names
        .iter()
        .filter_map(|name| manager.estimate_restore_impact(name))
        .fold(DiskImpact::empty(), |total, impact| total + impact);

    RollbackPreview {
        rows,
        impact,
        missing_names,
    }
}

fn prune_preview(names: &[String], manager: &RollbackManager<'_>) -> RollbackPreview {
    let rows = names
        .iter()
        .filter_map(|name| {
            let record = manager.rollback_record(name)?;
            let pkg = &record.package_snapshot;
            Some(RollbackPreviewRow {
                package: format!("{}/{}", pkg.provider, pkg.name),
                version: pkg.version.to_string(),
                net_change: manager
                    .estimate_prune_impact(name)
                    .map(|impact| impact.net)
                    .unwrap_or(SignedByteEstimate::exact(0)),
            })
        })
        .collect::<Vec<_>>();
    let missing_names = missing_names(names, &rows);
    let impact = names
        .iter()
        .filter_map(|name| manager.estimate_prune_impact(name))
        .fold(DiskImpact::empty(), |total, impact| total + impact);

    RollbackPreview {
        rows,
        impact,
        missing_names,
    }
}

fn missing_names(names: &[String], rows: &[RollbackPreviewRow]) -> Vec<String> {
    names
        .iter()
        .filter(|name| {
            !rows
                .iter()
                .any(|row| row.package.ends_with(&format!("/{name}")))
        })
        .cloned()
        .collect()
}

impl From<&RollbackPreviewRow> for output::TransactionRow {
    fn from(row: &RollbackPreviewRow) -> Self {
        output::TransactionRow::single_version(
            row.package.clone(),
            row.version.clone(),
            row.net_change,
            ByteEstimate::exact(0),
        )
    }
}
