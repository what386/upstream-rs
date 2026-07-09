use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::models::upstream::Package;
use crate::routines::doctor::checks::legacy;
use crate::routines::migrate::MigrationReport;
use crate::routines::migrate::step::Step;
use crate::services::integration::SymlinkManager;
use crate::storage::manifest::{CURRENT_LAYOUT_VERSION, ManifestStorage};
use crate::storage::rollback::RollbackRecord;
use crate::utils::filesystem::atomic_ops::write_atomic;
use crate::utils::filesystem::safe_move;
use crate::utils::static_paths::UpstreamPaths;

const PACKAGE_STORAGE_VERSION: u32 = 1;
const ROLLBACK_STORAGE_VERSION: u32 = 1;

#[derive(Debug, Clone)]
struct PathRewrite {
    old: PathBuf,
    new: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
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

pub struct V2_0_0;

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    V2_0_0::run(paths, report)
}

impl Step for V2_0_0 {
    fn check(paths: &UpstreamPaths) -> Result<bool> {
        Ok(legacy::legacy_package_dirs_exist(paths)
            || package_json_needs_path_rewrite(paths)?
            || rollback_metadata_needs_migration(paths)?
            || migration_manifest_needs_migration(paths)?)
    }

    fn apply(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
        let rewrites = package_path_rewrites(paths);
        let mut manifest_storage =
            ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))?;
        let previous_layout_version = manifest_storage
            .manifest()
            .map(|manifest| manifest.layout_version)
            .or_else(|| legacy::previous_layout_version_hint(paths));

        create_required_dirs(paths, report)?;
        move_legacy_package_dirs(&rewrites, report)?;
        let packages = migrate_package_json_metadata(paths, &rewrites, report)?;
        migrate_rollback_metadata(paths, &rewrites, report)?;
        refresh_symlinks(paths, &packages, report)?;
        manifest_storage.record_migration_from(previous_layout_version, CURRENT_LAYOUT_VERSION)?;

        Ok(())
    }
}

fn migration_manifest_needs_migration(paths: &UpstreamPaths) -> Result<bool> {
    let manifest_storage =
        ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))?;
    Ok(manifest_storage
        .manifest()
        .is_some_and(|manifest| manifest.layout_version < CURRENT_LAYOUT_VERSION))
}

fn rollback_metadata_needs_migration(paths: &UpstreamPaths) -> Result<bool> {
    let rollback_file = paths.dirs.metadata_dir.join("rollback.json");
    if !rollback_file.exists() {
        return Ok(false);
    }

    let json = fs::read_to_string(&rollback_file).with_context(|| {
        format!(
            "Failed to read rollback metadata '{}'",
            rollback_file.display()
        )
    })?;
    if json.trim().is_empty() {
        return Ok(false);
    }

    if serde_json::from_str::<LegacyRollbackStorageFile>(&json).is_ok() {
        return Ok(true);
    }

    let storage: RollbackStorageFile = serde_json::from_str(&json).with_context(|| {
        format!(
            "Failed to parse rollback metadata '{}'",
            rollback_file.display()
        )
    })?;
    let rewrites = package_path_rewrites(paths);
    Ok(storage.records.values().any(|records| {
        records
            .iter()
            .any(|record| package_references_rewrite(&record.package_snapshot, &rewrites))
    }))
}

fn create_required_dirs(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    for dir in [
        paths.dirs.config_dir.as_path(),
        paths.dirs.data_dir.as_path(),
        paths.dirs.packages_dir.as_path(),
        paths.dirs.cache_dir.as_path(),
        paths.dirs.metadata_dir.as_path(),
        paths.dirs.state_dir.as_path(),
        paths.install.appimages_dir.as_path(),
        paths.install.binaries_dir.as_path(),
        paths.install.archives_dir.as_path(),
        paths.install.tmp_dir.as_path(),
        paths.state.rollback_dir.as_path(),
        paths.state.icons_dir.as_path(),
        paths.state.symlinks_dir.as_path(),
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

fn migrate_package_json_metadata(
    paths: &UpstreamPaths,
    rewrites: &[PathRewrite],
    report: &mut MigrationReport,
) -> Result<Vec<Package>> {
    if !legacy::legacy_package_metadata_exists(paths) {
        return Ok(Vec::new());
    }

    let mut packages = legacy::load_legacy_package_metadata(paths)?;
    let mut changed = false;
    for package in &mut packages {
        let package_changed = rewrite_package_paths(package, rewrites);
        if package_changed {
            changed = true;
            report.updated_packages += 1;
        }
    }

    if changed {
        write_json(
            &paths.config.packages_file,
            &PackageStorageFile {
                version: PACKAGE_STORAGE_VERSION,
                packages: packages.clone(),
            },
        )?;
    }

    Ok(packages)
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

fn package_references_rewrite(package: &Package, rewrites: &[PathRewrite]) -> bool {
    optional_path_references_rewrite(package.install_path.as_deref(), rewrites)
        || optional_path_references_rewrite(package.exec_path.as_deref(), rewrites)
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

fn optional_path_references_rewrite(path: Option<&Path>, rewrites: &[PathRewrite]) -> bool {
    let Some(current) = path else {
        return false;
    };
    rewrites
        .iter()
        .any(|rewrite| current.starts_with(&rewrite.old))
}

fn package_json_needs_path_rewrite(paths: &UpstreamPaths) -> Result<bool> {
    if !legacy::legacy_package_metadata_exists(paths) {
        return Ok(false);
    }

    let rewrites = package_path_rewrites(paths);
    let packages = legacy::load_legacy_package_metadata(paths)?;
    Ok(packages
        .iter()
        .any(|package| package_references_rewrite(package, &rewrites)))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value).context("Failed to serialize migration data")?;
    write_atomic(path, json.as_bytes())
        .with_context(|| format!("Failed to write '{}'", path.display()))
}

fn refresh_symlinks(
    paths: &UpstreamPaths,
    packages: &[Package],
    report: &mut MigrationReport,
) -> Result<()> {
    if paths.dirs.data_dir.join("symlinks").exists() {
        return Ok(());
    }

    let symlink_manager = SymlinkManager::new(&paths.state.symlinks_dir);

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

#[cfg(test)]
mod tests {
    use super::{V2_0_0, run};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::routines::migrate::MigrationReport;
    use crate::routines::migrate::step::Step;
    use crate::storage::manifest::{
        CURRENT_LAYOUT_VERSION, MANIFEST_STORAGE_VERSION, ManifestStorage,
    };
    use crate::utils::test_support;
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-v2-0-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
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
    fn check_detects_legacy_package_dirs() {
        let root = temp_root("check-legacy-dirs");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(paths.dirs.data_dir.join("binaries")).expect("create legacy binaries");

        assert!(V2_0_0::check(&paths).expect("check migration"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn check_skips_empty_current_layout() {
        let root = temp_root("check-empty-current");
        let paths = test_support::upstream_paths(&root);

        assert!(!V2_0_0::check(&paths).expect("check migration"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_moves_package_dirs_and_rewrites_package_json() {
        let root = temp_root("layout");
        let paths = test_support::upstream_paths(&root);
        let old_binary = paths.dirs.data_dir.join("binaries").join("tool");
        let new_binary = paths.dirs.packages_dir.join("binaries").join("tool");
        fs::create_dir_all(old_binary.parent().expect("old binary parent"))
            .expect("create old binary parent");
        fs::write(&old_binary, b"tool").expect("write old binary");

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
        let mut report = MigrationReport::default();

        run(&paths, &mut report).expect("migrate package layout");

        assert!(!old_binary.exists());
        assert_eq!(
            fs::read(&new_binary).expect("read migrated binary"),
            b"tool"
        );
        assert_eq!(report.updated_packages, 1);

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
        let migration_manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(ManifestStorage::path_for_root(&paths.dirs.data_dir))
                .expect("read migration manifest"),
        )
        .expect("parse migration manifest");
        assert_eq!(
            migration_manifest["manifest_version"].as_u64(),
            Some(MANIFEST_STORAGE_VERSION as u64)
        );
        assert_eq!(
            migration_manifest["layout_version"].as_u64(),
            Some(CURRENT_LAYOUT_VERSION as u64)
        );
        assert_eq!(
            migration_manifest["previous_layout_version"].as_u64(),
            Some(1)
        );
        assert!(!V2_0_0::check(&paths).expect("check after migration"));

        cleanup(&root).expect("cleanup");
    }
}
