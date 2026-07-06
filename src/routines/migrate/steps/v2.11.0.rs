use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::models::upstream::Package;
use crate::routines::migrate::MigrationReport;
use crate::services::integration::SymlinkManager;
use crate::storage::database::PackageDatabase;
use crate::storage::rollback::RollbackRecord;
use crate::utils::filesystem::atomic_ops::write_atomic;
use crate::utils::filesystem::safe_move;
use crate::utils::static_paths::UpstreamPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RollbackStorageFile {
    version: u32,
    records: HashMap<String, Vec<RollbackRecord>>,
}

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    let old_rollback_dir = paths.dirs.data_dir.join("rollback");
    let old_symlinks_dir = paths.dirs.data_dir.join("symlinks");
    let old_icons_dir = paths.dirs.data_dir.join("icons");

    create_state_directories(paths, report)?;
    move_legacy_state_dir(&old_rollback_dir, &paths.state.rollback_dir, report)?;
    move_legacy_state_dir(&old_symlinks_dir, &paths.state.symlinks_dir, report)?;
    move_legacy_state_dir(&old_icons_dir, &paths.state.icons_dir, report)?;

    rewrite_paths_file(
        &paths.config.paths_file,
        &old_symlinks_dir,
        &paths.state.symlinks_dir,
    )?;
    rewrite_paths_file(
        &paths.config.paths_nu_file,
        &old_symlinks_dir,
        &paths.state.symlinks_dir,
    )?;
    rewrite_package_database_icons(paths, &old_icons_dir, report)?;
    rewrite_rollback_storage(paths, &old_icons_dir, report)?;
    rewrite_desktop_entries(paths, &old_icons_dir, &paths.state.icons_dir)?;
    refresh_symlinks(paths, report)?;

    Ok(())
}

fn create_state_directories(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    for dir in [
        paths.dirs.state_dir.as_path(),
        paths.state.rollback_dir.as_path(),
        paths.state.symlinks_dir.as_path(),
        paths.state.icons_dir.as_path(),
    ] {
        if !dir.exists() {
            report.created_dirs += 1;
        }
        fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create directory '{}'", dir.display()))?;
    }
    Ok(())
}

fn move_legacy_state_dir(src: &Path, dst: &Path, report: &mut MigrationReport) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }

    if same_location(src, dst)? {
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
            if paths_are_equivalent(&from, &to)? {
                remove_file_or_dir(&from)?;
                continue;
            }
            return Err(anyhow::anyhow!(
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

fn same_location(src: &Path, dst: &Path) -> Result<bool> {
    if !src.exists() || !dst.exists() {
        return Ok(false);
    }
    Ok(fs::canonicalize(src)? == fs::canonicalize(dst)?)
}

fn paths_are_equivalent(src: &Path, dst: &Path) -> Result<bool> {
    let src_metadata = fs::symlink_metadata(src)
        .with_context(|| format!("Failed to inspect '{}'", src.display()))?;
    let dst_metadata = fs::symlink_metadata(dst)
        .with_context(|| format!("Failed to inspect '{}'", dst.display()))?;

    if src_metadata.file_type().is_symlink() || dst_metadata.file_type().is_symlink() {
        return Ok(src_metadata.file_type().is_symlink()
            && dst_metadata.file_type().is_symlink()
            && fs::read_link(src)
                .with_context(|| format!("Failed to read symlink '{}'", src.display()))?
                == fs::read_link(dst)
                    .with_context(|| format!("Failed to read symlink '{}'", dst.display()))?);
    }

    if src_metadata.is_file() && dst_metadata.is_file() {
        return Ok(
            fs::read(src).with_context(|| format!("Failed to read '{}'", src.display()))?
                == fs::read(dst).with_context(|| format!("Failed to read '{}'", dst.display()))?,
        );
    }

    Ok(false)
}

fn remove_file_or_dir(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Failed to inspect '{}'", path.display()))?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
            .with_context(|| format!("Failed to remove duplicate path '{}'", path.display()))?;
    } else if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| {
            format!("Failed to remove duplicate directory '{}'", path.display())
        })?;
    }
    Ok(())
}

fn rewrite_paths_file(path: &Path, old_path: &Path, new_path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let old_value = old_path.display().to_string();
    let new_value = new_path.display().to_string();
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read paths file '{}'", path.display()))?;
    let updated = contents.replace(&old_value, &new_value);
    if updated != contents {
        write_atomic(path, updated.as_bytes())
            .with_context(|| format!("Failed to write paths file '{}'", path.display()))?;
    }
    Ok(())
}

fn rewrite_package_database_icons(
    paths: &UpstreamPaths,
    old_icons_dir: &Path,
    report: &mut MigrationReport,
) -> Result<()> {
    let mut database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut packages = database.list_packages()?;
    let mut changed = false;
    let mut updated_packages = 0;

    for package in &mut packages {
        if rewrite_package_icon_path(package, old_icons_dir, &paths.state.icons_dir) {
            changed = true;
            updated_packages += 1;
        }
    }

    if changed {
        database.replace_all_packages(&packages)?;
        report.updated_packages += updated_packages;
    }

    Ok(())
}

fn rewrite_package_icon_path(
    package: &mut Package,
    old_icons_dir: &Path,
    new_icons_dir: &Path,
) -> bool {
    let Some(icon_path) = package.icon_path.as_ref() else {
        return false;
    };
    let Ok(relative) = icon_path.strip_prefix(old_icons_dir) else {
        return false;
    };
    package.icon_path = Some(new_icons_dir.join(relative));
    true
}

fn rewrite_rollback_storage(
    paths: &UpstreamPaths,
    old_icons_dir: &Path,
    report: &mut MigrationReport,
) -> Result<()> {
    let rollback_file = paths.dirs.metadata_dir.join("rollback.json");
    if !rollback_file.exists() {
        return Ok(());
    }

    let json = fs::read_to_string(&rollback_file).with_context(|| {
        format!(
            "Failed to read rollback storage '{}'",
            rollback_file.display()
        )
    })?;
    if json.trim().is_empty() {
        return Ok(());
    }

    let mut storage: RollbackStorageFile = serde_json::from_str(&json).with_context(|| {
        format!(
            "Failed to parse rollback storage '{}'",
            rollback_file.display()
        )
    })?;

    let mut changed = false;
    let mut updated_records = 0;
    for records in storage.records.values_mut() {
        for record in records {
            if rewrite_package_icon_path(
                &mut record.package_snapshot,
                old_icons_dir,
                &paths.state.icons_dir,
            ) {
                changed = true;
                updated_records += 1;
            }
        }
    }

    if changed {
        let updated_json = serde_json::to_string_pretty(&storage)
            .context("Failed to serialize rollback storage")?;
        write_atomic(&rollback_file, updated_json.as_bytes()).with_context(|| {
            format!(
                "Failed to write rollback storage '{}'",
                rollback_file.display()
            )
        })?;
        report.updated_rollback_records += updated_records;
    }

    Ok(())
}

fn rewrite_desktop_entries(
    paths: &UpstreamPaths,
    old_icons_dir: &Path,
    new_icons_dir: &Path,
) -> Result<()> {
    let applications_dir = &paths.integration.xdg_applications_dir;
    if !applications_dir.exists() {
        return Ok(());
    }

    let old_value = old_icons_dir.display().to_string();
    let new_value = new_icons_dir.display().to_string();
    for entry in fs::read_dir(applications_dir).with_context(|| {
        format!(
            "Failed to read desktop entry directory '{}'",
            applications_dir.display()
        )
    })? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
            continue;
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read desktop entry '{}'", path.display()))?;
        let updated = contents.replace(&old_value, &new_value);
        if updated != contents {
            write_atomic(&path, updated.as_bytes())
                .with_context(|| format!("Failed to update desktop entry '{}'", path.display()))?;
        }
    }

    Ok(())
}

fn refresh_symlinks(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    if !paths.config.packages_database_file.exists() {
        return Ok(());
    }

    let database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let packages = database.list_packages()?;
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
    use super::run;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::routines::migrate::MigrationReport;
    use crate::storage::database::PackageDatabase;
    use crate::utils::test_support;
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-v2-11-test", name)
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
    fn migrate_moves_state_directories_and_rewrites_references() {
        let root = temp_root("state-layout");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config dir");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");

        let old_rollback_dir = paths.dirs.data_dir.join("rollback");
        let old_symlinks_dir = paths.dirs.data_dir.join("symlinks");
        let old_icons_dir = paths.dirs.data_dir.join("icons");
        fs::create_dir_all(&old_rollback_dir).expect("create rollback dir");
        fs::create_dir_all(&old_symlinks_dir).expect("create symlinks dir");
        fs::create_dir_all(&old_icons_dir).expect("create icons dir");
        fs::write(old_symlinks_dir.join("tool"), b"link").expect("write symlink placeholder");
        fs::write(old_icons_dir.join("tool.png"), b"icon").expect("write icon");

        let mut package_db = PackageDatabase::open(&paths.config.packages_database_file)
            .expect("create package database");
        let mut package = test_package(
            "tool",
            paths.dirs.packages_dir.join("binaries/tool"),
            paths.dirs.packages_dir.join("binaries/tool"),
        );
        package.icon_path = Some(old_icons_dir.join("tool.png"));
        package_db
            .replace_all_packages(&[package.clone()])
            .expect("seed database");

        let rollback_file = paths.dirs.metadata_dir.join("rollback.json");
        fs::create_dir_all(rollback_file.parent().expect("rollback parent"))
            .expect("create rollback parent");
        fs::write(
            &rollback_file,
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "records": {
                    "tool": [{
                        "package_snapshot": package,
                        "artifact_relative_path": "tool/artifact.tgz",
                        "icon_relative_path": null,
                        "artifact_format": "tgz",
                        "artifact_entry_path": null,
                        "icon_entry_path": null,
                        "source": "Upgrade",
                        "created_at": "2026-07-06T00:00:00Z"
                    }]
                }
            }))
            .expect("serialize rollback"),
        )
        .expect("write rollback");

        fs::create_dir_all(paths.config.paths_file.parent().expect("paths parent"))
            .expect("create paths parent");
        fs::write(
            &paths.config.paths_file,
            format!("export PATH=\"{}:$PATH\"\n", old_symlinks_dir.display()),
        )
        .expect("write paths.sh");
        fs::write(
            &paths.config.paths_nu_file,
            format!(
                "$env.PATH = ($env.PATH | prepend '{}')\n",
                old_symlinks_dir.display()
            ),
        )
        .expect("write paths.nu");

        fs::create_dir_all(&paths.integration.xdg_applications_dir).expect("create desktop dir");
        fs::write(
            paths.integration.xdg_applications_dir.join("tool.desktop"),
            format!(
                "[Desktop Entry]\nIcon={}\n",
                old_icons_dir.join("tool.png").display()
            ),
        )
        .expect("write desktop entry");

        let mut report = MigrationReport::default();
        run(&paths, &mut report).expect("run migration");

        assert!(!old_rollback_dir.exists());
        assert!(!old_symlinks_dir.exists());
        assert!(!old_icons_dir.exists());
        assert!(paths.state.rollback_dir.exists());
        assert!(paths.state.symlinks_dir.exists());
        assert!(paths.state.icons_dir.exists());
        assert_eq!(report.moved_entries, 2);
        assert!(report.updated_packages >= 1);
        assert_eq!(report.updated_rollback_records, 1);

        let migrated_config = fs::read_to_string(&paths.config.paths_file).expect("read paths.sh");
        assert!(migrated_config.contains(&paths.state.symlinks_dir.display().to_string()));
        let migrated_nu = fs::read_to_string(&paths.config.paths_nu_file).expect("read paths.nu");
        assert!(migrated_nu.contains(&paths.state.symlinks_dir.display().to_string()));
        let migrated_db = PackageDatabase::open(&paths.config.packages_database_file)
            .expect("reopen package database");
        let migrated_package = migrated_db
            .get_package("tool")
            .expect("load migrated package")
            .expect("package exists");
        let expected_icon_path = paths.state.icons_dir.join("tool.png");
        assert_eq!(
            migrated_package.icon_path.as_deref(),
            Some(expected_icon_path.as_path())
        );
        let migrated_rollback = fs::read_to_string(&rollback_file).expect("read rollback");
        let migrated_rollback: serde_json::Value =
            serde_json::from_str(&migrated_rollback).expect("parse rollback");
        let rollback_icon_path = migrated_rollback
            .get("records")
            .and_then(|records| records.get("tool"))
            .and_then(|records| records.as_array())
            .and_then(|records| records.first())
            .and_then(|record| record.get("package_snapshot"))
            .and_then(|snapshot| snapshot.get("icon_path"))
            .and_then(|icon_path| icon_path.as_str())
            .expect("rollback icon path");
        assert!(rollback_icon_path.contains(&paths.state.icons_dir.display().to_string()));
        let migrated_desktop =
            fs::read_to_string(paths.integration.xdg_applications_dir.join("tool.desktop"))
                .expect("read desktop");
        assert!(migrated_desktop.contains(&paths.state.icons_dir.display().to_string()));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_treats_existing_equivalent_state_entries_as_already_moved() {
        let root = temp_root("duplicate-state-entry");
        let paths = test_support::upstream_paths(&root);
        let old_symlinks_dir = paths.dirs.data_dir.join("symlinks");
        fs::create_dir_all(&old_symlinks_dir).expect("create old symlinks");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create state symlinks");
        fs::write(old_symlinks_dir.join("tool"), b"same target").expect("write old link");
        fs::write(paths.state.symlinks_dir.join("tool"), b"same target")
            .expect("write refreshed link");

        let mut report = MigrationReport::default();
        run(&paths, &mut report).expect("run migration");

        assert!(!old_symlinks_dir.exists());
        assert_eq!(
            fs::read(paths.state.symlinks_dir.join("tool")).expect("read kept link"),
            b"same target"
        );

        cleanup(&root).expect("cleanup");
    }
}
