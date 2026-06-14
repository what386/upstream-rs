use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::models::upstream::Package;
use crate::services::integration::SymlinkManager;
use crate::services::storage::rollback_storage::RollbackRecord;
use crate::utils::filesystem::{atomic_ops::write_atomic, safe_move};
use crate::utils::static_paths::UpstreamPaths;

const PACKAGE_STORAGE_VERSION: u32 = 1;
const ROLLBACK_STORAGE_VERSION: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub created_dirs: usize,
    pub moved_entries: usize,
    pub updated_packages: usize,
    pub updated_rollback_records: usize,
    pub refreshed_symlinks: usize,
    pub skipped_symlinks: usize,
}

#[derive(Debug, Clone)]
struct PathRewrite {
    old: PathBuf,
    new: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageStorageFile {
    version: u32,
    packages: Vec<Package>,
}

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

pub fn run(paths: &UpstreamPaths) -> Result<MigrationReport> {
    let rewrites = package_path_rewrites(paths);
    let mut report = MigrationReport::default();

    create_required_dirs(paths, &mut report)?;
    move_legacy_package_dirs(&rewrites, &mut report)?;
    let packages = migrate_package_metadata(paths, &rewrites, &mut report)?;
    migrate_rollback_metadata(paths, &rewrites, &mut report)?;
    refresh_symlinks(paths, &packages, &mut report)?;

    Ok(report)
}

fn create_required_dirs(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    for dir in [
        paths.dirs.config_dir.as_path(),
        paths.dirs.data_dir.as_path(),
        paths.dirs.packages_dir.as_path(),
        paths.dirs.cache_dir.as_path(),
        paths.dirs.metadata_dir.as_path(),
        paths.install.appimages_dir.as_path(),
        paths.install.binaries_dir.as_path(),
        paths.install.archives_dir.as_path(),
        paths.install.rollback_dir.as_path(),
        paths.install.tmp_dir.as_path(),
        paths.integration.icons_dir.as_path(),
        paths.integration.symlinks_dir.as_path(),
    ] {
        if !dir.exists() {
            report.created_dirs += 1;
        }
        fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create directory '{}'", dir.display()))?;
    }
    Ok(())
}

fn package_path_rewrites(paths: &UpstreamPaths) -> Vec<PathRewrite> {
    vec![
        PathRewrite {
            old: paths.dirs.data_dir.join("appimages"),
            new: paths.install.appimages_dir.clone(),
        },
        PathRewrite {
            old: paths.dirs.data_dir.join("binaries"),
            new: paths.install.binaries_dir.clone(),
        },
        PathRewrite {
            old: paths.dirs.data_dir.join("archives"),
            new: paths.install.archives_dir.clone(),
        },
    ]
}

fn move_legacy_package_dirs(rewrites: &[PathRewrite], report: &mut MigrationReport) -> Result<()> {
    for rewrite in rewrites {
        if !rewrite.old.exists() {
            continue;
        }
        move_into_layout(&rewrite.old, &rewrite.new, report).with_context(|| {
            format!(
                "Failed to migrate '{}' to '{}'",
                rewrite.old.display(),
                rewrite.new.display()
            )
        })?;
    }
    Ok(())
}

fn move_into_layout(src: &Path, dst: &Path, report: &mut MigrationReport) -> Result<()> {
    if paths_are_same(src, dst)? {
        return Ok(());
    }

    if !dst.exists() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
        }
        safe_move::move_file_or_dir(src, dst)?;
        report.moved_entries += 1;
        return Ok(());
    }

    merge_directory_contents(src, dst, report)?;
    remove_dir_if_empty(src)?;
    Ok(())
}

fn merge_directory_contents(src: &Path, dst: &Path, report: &mut MigrationReport) -> Result<()> {
    for entry in fs::read_dir(src)
        .with_context(|| format!("Failed to read directory '{}'", src.display()))?
    {
        let entry =
            entry.with_context(|| format!("Failed to read entry in '{}'", src.display()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to inspect '{}'", from.display()))?;

        if to.exists() {
            if file_type.is_dir() && to.is_dir() {
                merge_directory_contents(&from, &to, report)?;
                remove_dir_if_empty(&from)?;
                continue;
            }
            return Err(anyhow!(
                "Refusing to overwrite existing migrated path '{}'",
                to.display()
            ));
        }

        safe_move::move_file_or_dir(&from, &to)?;
        report.moved_entries += 1;
    }
    Ok(())
}

fn remove_dir_if_empty(path: &Path) -> Result<()> {
    if path.exists()
        && path
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    {
        fs::remove_dir(path)
            .with_context(|| format!("Failed to remove empty directory '{}'", path.display()))?;
    }
    Ok(())
}

fn paths_are_same(a: &Path, b: &Path) -> io::Result<bool> {
    if !a.exists() || !b.exists() {
        return Ok(false);
    }
    Ok(fs::canonicalize(a)? == fs::canonicalize(b)?)
}

fn migrate_package_metadata(
    paths: &UpstreamPaths,
    rewrites: &[PathRewrite],
    report: &mut MigrationReport,
) -> Result<Vec<Package>> {
    if !paths.config.packages_file.exists() {
        return Ok(Vec::new());
    }

    let json = fs::read_to_string(&paths.config.packages_file).with_context(|| {
        format!(
            "Failed to read package metadata '{}'",
            paths.config.packages_file.display()
        )
    })?;
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut storage: PackageStorageFile = serde_json::from_str(&json).with_context(|| {
        format!(
            "Failed to parse package metadata '{}'",
            paths.config.packages_file.display()
        )
    })?;
    if storage.version != PACKAGE_STORAGE_VERSION {
        return Err(anyhow!(
            "Unsupported package storage version {} in '{}'. Expected version {}.",
            storage.version,
            paths.config.packages_file.display(),
            PACKAGE_STORAGE_VERSION
        ));
    }

    let mut changed = false;
    for package in &mut storage.packages {
        let package_changed = rewrite_package_paths(package, rewrites);
        if package_changed {
            changed = true;
            report.updated_packages += 1;
        }
    }

    if changed {
        write_json(&paths.config.packages_file, &storage)?;
    }

    Ok(storage.packages)
}

fn migrate_rollback_metadata(
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

fn refresh_symlinks(
    paths: &UpstreamPaths,
    packages: &[Package],
    report: &mut MigrationReport,
) -> Result<()> {
    let symlink_manager = SymlinkManager::new(&paths.integration.symlinks_dir);

    for package in packages {
        let target = package.exec_path.as_ref().or(package.install_path.as_ref());
        let Some(target) = target else {
            report.skipped_symlinks += 1;
            continue;
        };
        if !target.exists() {
            report.skipped_symlinks += 1;
            continue;
        }

        symlink_manager
            .add_link(target, &package.name)
            .with_context(|| format!("Failed to refresh symlink for '{}'", package.name))?;
        report.refreshed_symlinks += 1;
    }

    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value).context("Failed to serialize migration data")?;
    write_atomic(path, json.as_bytes())
        .with_context(|| format!("Failed to write '{}'", path.display()))
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::services::storage::rollback_storage::{
        RollbackArtifactFormat, RollbackRecord, RollbackSource,
    };
    use crate::utils::test_support;
    use chrono::Utc;
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn test_package(name: &str, install_path: PathBuf, exec_path: PathBuf) -> Package {
        let mut package = Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(install_path);
        package.exec_path = Some(exec_path);
        package
    }

    #[test]
    fn migrate_moves_package_dirs_and_rewrites_metadata() {
        let root = temp_root("layout");
        let paths = test_support::upstream_paths(&root);
        let old_binary = paths.dirs.data_dir.join("binaries").join("tool");
        let new_binary = paths.dirs.packages_dir.join("binaries").join("tool");
        fs::create_dir_all(old_binary.parent().expect("old binary parent"))
            .expect("create old binary parent");
        fs::write(&old_binary, b"tool").expect("write old binary");

        #[cfg(unix)]
        {
            fs::create_dir_all(&paths.integration.symlinks_dir).expect("create symlinks");
            std::os::unix::fs::symlink(&old_binary, paths.integration.symlinks_dir.join("tool"))
                .expect("create old symlink");
        }

        let package = test_package("tool", old_binary.clone(), old_binary.clone());
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");
        fs::write(
            &paths.config.packages_file,
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "packages": [package],
            }))
            .expect("serialize packages"),
        )
        .expect("write packages");

        let report = run(&paths).expect("migrate");

        assert!(!old_binary.exists());
        assert_eq!(
            fs::read(&new_binary).expect("read migrated binary"),
            b"tool"
        );
        assert_eq!(report.updated_packages, 1);
        assert_eq!(report.refreshed_symlinks, 1);

        let migrated: serde_json::Value = serde_json::from_slice(
            &fs::read(&paths.config.packages_file).expect("read migrated packages"),
        )
        .expect("parse migrated packages");
        assert_eq!(
            migrated["packages"][0]["install_path"].as_str(),
            Some(new_binary.to_str().expect("utf8 path"))
        );
        assert_eq!(
            migrated["packages"][0]["exec_path"].as_str(),
            Some(new_binary.to_str().expect("utf8 path"))
        );

        #[cfg(unix)]
        assert_eq!(
            fs::read_link(paths.integration.symlinks_dir.join("tool")).expect("read symlink"),
            new_binary
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_rewrites_rollback_package_snapshots() {
        let root = temp_root("rollback");
        let paths = test_support::upstream_paths(&root);
        let old_archive = paths
            .dirs
            .data_dir
            .join("archives")
            .join("tool")
            .join("bin")
            .join("tool");
        let new_archive = paths
            .dirs
            .packages_dir
            .join("archives")
            .join("tool")
            .join("bin")
            .join("tool");
        fs::create_dir_all(old_archive.parent().expect("old archive parent"))
            .expect("create old archive parent");
        fs::write(&old_archive, b"tool").expect("write old archive executable");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");

        let package = test_package(
            "tool",
            paths.dirs.data_dir.join("archives").join("tool"),
            old_archive.clone(),
        );
        let record = RollbackRecord {
            package_snapshot: package,
            artifact_relative_path: PathBuf::from("tool/archive.tgz"),
            icon_relative_path: None,
            artifact_format: RollbackArtifactFormat::Tgz,
            artifact_entry_path: Some(PathBuf::from("artifact/tool")),
            icon_entry_path: None,
            source: RollbackSource::Upgrade,
            created_at: Utc::now(),
        };
        fs::write(
            paths.dirs.metadata_dir.join("rollback.json"),
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "records": {
                    "tool": [record],
                },
            }))
            .expect("serialize rollback"),
        )
        .expect("write rollback");

        let report = run(&paths).expect("migrate");

        assert_eq!(
            fs::read(&new_archive).expect("read migrated archive"),
            b"tool"
        );
        assert_eq!(report.updated_rollback_records, 1);
        let migrated: serde_json::Value = serde_json::from_slice(
            &fs::read(paths.dirs.metadata_dir.join("rollback.json")).expect("read rollback"),
        )
        .expect("parse rollback");
        assert_eq!(
            migrated["records"]["tool"][0]["package_snapshot"]["install_path"].as_str(),
            Some(
                paths
                    .dirs
                    .packages_dir
                    .join("archives")
                    .join("tool")
                    .to_str()
                    .expect("utf8 path")
            )
        );
        assert_eq!(
            migrated["records"]["tool"][0]["package_snapshot"]["exec_path"].as_str(),
            Some(new_archive.to_str().expect("utf8 path"))
        );

        cleanup(&root).expect("cleanup");
    }
}
