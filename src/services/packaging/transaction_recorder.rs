use anyhow::Result;

use crate::services::storage::transaction_storage::{
    TransactionKind, TransactionLog, TransactionPackage, UndoActionKind, package_failed,
    package_skipped, package_success, planned_packages, undo,
};
use crate::utils::static_paths::UpstreamPaths;

pub struct PackageTransaction {
    log: TransactionLog,
}

impl PackageTransaction {
    pub fn start(
        paths: &UpstreamPaths,
        kind: TransactionKind,
        package_names: Vec<String>,
        undo_kind: Option<UndoActionKind>,
    ) -> Result<Self> {
        let undo_action = undo_kind.and_then(|kind| undo(kind, package_names.clone()));
        Ok(Self {
            log: TransactionLog::start(paths, kind, planned_packages(package_names), undo_action)?,
        })
    }

    pub fn complete(self, packages: Vec<TransactionPackage>) -> Result<()> {
        self.log.complete(packages)
    }

    pub fn fail(self, packages: Vec<TransactionPackage>, error: impl Into<String>) -> Result<()> {
        self.log.fail(packages, error)
    }
}

pub fn successful_package(
    name: impl Into<String>,
    old_version: Option<String>,
    new_version: Option<String>,
) -> TransactionPackage {
    let mut package = package_success(name);
    package.old_version = old_version;
    package.new_version = new_version;
    package
}

pub fn failed_package(
    name: impl Into<String>,
    old_version: Option<String>,
    new_version: Option<String>,
    error: impl Into<String>,
) -> TransactionPackage {
    let mut package = package_failed(name, error);
    package.old_version = old_version;
    package.new_version = new_version;
    package
}

pub fn skipped_package(
    name: impl Into<String>,
    old_version: Option<String>,
    new_version: Option<String>,
    reason: impl Into<String>,
) -> TransactionPackage {
    let mut package = package_skipped(name, reason);
    package.old_version = old_version;
    package.new_version = new_version;
    package
}
