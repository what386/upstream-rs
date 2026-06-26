use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::models::upstream::Package;
use crate::routines::doctor::checks::legacy;
use crate::routines::migrate::MigrationReport;
use crate::routines::migrate::layout::PathRewrite;
use crate::storage::database::PackageDatabase;
use crate::storage::rollback::RollbackRecord;
use crate::utils::filesystem::atomic_ops::write_atomic;
use crate::utils::static_paths::UpstreamPaths;

const ROLLBACK_STORAGE_VERSION: u32 = 1;
const PACKAGE_DB_SCHEMA_VERSION_WITH_TAG_TEMPLATE: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RollbackStorageFile {
    version: u32,
    records: HashMap<String, Vec<RollbackRecord>>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyRollbackStorageFile {
    version: u32,
    records: HashMap<String, RollbackRecord>,
}

pub(in crate::routines::migrate) fn migrate_package_metadata(
    paths: &UpstreamPaths,
    rewrites: &[PathRewrite],
    report: &mut MigrationReport,
) -> Result<Vec<Package>> {
    let database_exists = paths.config.packages_database_file.exists();
    if database_exists {
        migrate_package_database_schema(paths)?;
    }
    let mut storage = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut packages = if !database_exists && legacy::legacy_package_metadata_exists(paths) {
        let packages = legacy::load_legacy_package_metadata(paths)?;
        storage.replace_all_packages(&packages)?;
        packages
    } else {
        storage.list_packages()?
    };

    let mut changed = false;
    for package in &mut packages {
        let package_changed = rewrite_package_paths(package, rewrites);
        if package_changed {
            changed = true;
            report.updated_packages += 1;
        }
    }

    if changed {
        storage.replace_all_packages(&packages)?;
    }

    Ok(packages)
}

fn migrate_package_database_schema(paths: &UpstreamPaths) -> Result<()> {
    let conn = Connection::open(&paths.config.packages_database_file).with_context(|| {
        format!(
            "Failed to open package database '{}'",
            paths.config.packages_database_file.display()
        )
    })?;
    let schema_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .context("Failed to read package database schema version")?;

    if schema_version == PACKAGE_DB_SCHEMA_VERSION_WITH_TAG_TEMPLATE {
        return Ok(());
    }
    if schema_version != 1 {
        return Err(anyhow!(
            "Unsupported package database schema version {} in '{}'. Expected version 1 or {}.",
            schema_version,
            paths.config.packages_database_file.display(),
            PACKAGE_DB_SCHEMA_VERSION_WITH_TAG_TEMPLATE
        ));
    }

    conn.execute_batch(
        "ALTER TABLE packages ADD COLUMN version_tag_template TEXT;
         PRAGMA user_version = 2;",
    )
    .context("Failed to migrate package database schema to version 2")
}

pub(in crate::routines::migrate) fn migrate_rollback_metadata(
    paths: &UpstreamPaths,
    rewrites: &[PathRewrite],
    report: &mut MigrationReport,
) -> Result<()> {
    let rollback_file = paths.dirs.metadata_dir.join("rollback.json");
    if !rollback_file.exists() {
        return Ok(());
    }

    let json = fs::read_to_string(&rollback_file).with_context(|| {
        format!(
            "Failed to read rollback metadata '{}'",
            rollback_file.display()
        )
    })?;
    if json.trim().is_empty() {
        return Ok(());
    }

    let mut storage: RollbackStorageFile = serde_json::from_str(&json)
        .or_else(|_| parse_legacy_rollback_storage(&json))
        .with_context(|| {
            format!(
                "Failed to parse rollback metadata '{}'",
                rollback_file.display()
            )
        })?;
    if storage.version != ROLLBACK_STORAGE_VERSION {
        return Err(anyhow!(
            "Unsupported rollback storage version {} in '{}'. Expected version {}.",
            storage.version,
            rollback_file.display(),
            ROLLBACK_STORAGE_VERSION
        ));
    }

    let mut changed = false;
    for records in storage.records.values_mut() {
        for record in records {
            if rewrite_package_paths(&mut record.package_snapshot, rewrites) {
                changed = true;
                report.updated_rollback_records += 1;
            }
        }
    }

    if changed {
        write_json(&rollback_file, &storage)?;
    }

    Ok(())
}

fn parse_legacy_rollback_storage(json: &str) -> serde_json::Result<RollbackStorageFile> {
    let legacy: LegacyRollbackStorageFile = serde_json::from_str(json)?;
    Ok(RollbackStorageFile {
        version: legacy.version,
        records: legacy
            .records
            .into_iter()
            .map(|(name, record)| (name, vec![record]))
            .collect(),
    })
}

fn rewrite_package_paths(package: &mut Package, rewrites: &[PathRewrite]) -> bool {
    let mut changed = false;
    changed |= rewrite_optional_path(&mut package.install_path, rewrites);
    changed |= rewrite_optional_path(&mut package.exec_path, rewrites);
    changed
}

fn rewrite_optional_path(path: &mut Option<PathBuf>, rewrites: &[PathRewrite]) -> bool {
    let Some(current) = path.as_ref() else {
        return false;
    };

    for rewrite in rewrites {
        if let Ok(relative) = current.strip_prefix(&rewrite.old) {
            *path = Some(rewrite.new.join(relative));
            return true;
        }
    }

    false
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value).context("Failed to serialize migration data")?;
    write_atomic(path, json.as_bytes())
        .with_context(|| format!("Failed to write '{}'", path.display()))
}
