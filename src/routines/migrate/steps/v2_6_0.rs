use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::routines::doctor::checks::legacy;
use crate::routines::migrate::MigrationReport;
use crate::routines::migrate::step::Step;
use crate::services::integration::SymlinkManager;
use crate::storage::database::{PACKAGE_DB_SCHEMA_VERSION, PackageDatabase};
use crate::utils::static_paths::UpstreamPaths;

pub struct V2_6_0;

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    V2_6_0::run(paths, report)
}

impl Step for V2_6_0 {
    fn check(paths: &UpstreamPaths) -> Result<bool> {
        Ok(package_database_schema_needs_migration(paths)?
            || legacy_packages_need_database_import(paths)?)
    }

    fn apply(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
        let database_existed = paths.config.packages_database_file.exists();
        let mut storage = PackageDatabase::open(&paths.config.packages_database_file)?;
        let packages = storage.list_packages()?;
        let packages = if legacy::legacy_package_metadata_exists(paths) && packages.is_empty() {
            let legacy_packages = legacy::load_legacy_package_metadata(paths)?;
            if !legacy_packages.is_empty() {
                storage.replace_all_packages(&legacy_packages)?;
            }
            legacy_packages
        } else {
            packages
        };

        if database_existed {
            refresh_symlinks(paths, &packages, report)?;
        }

        Ok(())
    }
}

fn package_database_schema_needs_migration(paths: &UpstreamPaths) -> Result<bool> {
    if !paths.config.packages_database_file.exists() {
        return Ok(false);
    }

    let conn = Connection::open(&paths.config.packages_database_file).with_context(|| {
        format!(
            "Failed to open package database '{}'",
            paths.config.packages_database_file.display()
        )
    })?;
    let schema_version = conn
        .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .context("Failed to read package database schema version")?;

    Ok(schema_version < PACKAGE_DB_SCHEMA_VERSION)
}

fn legacy_packages_need_database_import(paths: &UpstreamPaths) -> Result<bool> {
    if !legacy::legacy_package_metadata_exists(paths) {
        return Ok(false);
    }
    if !paths.config.packages_database_file.exists() {
        return Ok(true);
    }

    let conn = Connection::open(&paths.config.packages_database_file).with_context(|| {
        format!(
            "Failed to open package database '{}'",
            paths.config.packages_database_file.display()
        )
    })?;
    let package_count = conn
        .query_row("SELECT COUNT(*) FROM packages", [], |row| {
            row.get::<_, u32>(0)
        })
        .unwrap_or(0);
    Ok(package_count == 0)
}

fn refresh_symlinks(
    paths: &UpstreamPaths,
    packages: &[crate::models::upstream::Package],
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
    use super::{V2_6_0, run};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::routines::migrate::MigrationReport;
    use crate::routines::migrate::step::Step;
    use crate::storage::database::PackageDatabase;
    use crate::utils::test_support;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-v2-6-test", name)
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
    fn check_detects_legacy_package_json_without_database() {
        let root = temp_root("check-package-json");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");
        fs::write(
            &paths.config.packages_file,
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": 1,
                "packages": [],
            }))
            .expect("serialize packages"),
        )
        .expect("write packages");

        assert!(V2_6_0::check(&paths).expect("check migration"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_imports_package_json_into_database() {
        let root = temp_root("package-json-import");
        let paths = test_support::upstream_paths(&root);
        let binary = paths.dirs.packages_dir.join("binaries").join("tool");
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("create binary parent");
        fs::write(&binary, b"tool").expect("write binary");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");
        fs::write(
            &paths.config.packages_file,
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": 1,
                "packages": [test_package("tool", binary.clone(), binary.clone())],
            }))
            .expect("serialize packages"),
        )
        .expect("write packages");
        let mut report = MigrationReport::default();

        run(&paths, &mut report).expect("migrate package database");

        let migrated_storage = PackageDatabase::open(&paths.config.packages_database_file)
            .expect("open migrated package database");
        let migrated_package = migrated_storage
            .get_package("tool")
            .expect("load migrated package")
            .expect("migrated package");
        assert_eq!(
            migrated_package.install_path.as_deref(),
            Some(binary.as_path())
        );
        assert_eq!(
            migrated_package.exec_path.as_deref(),
            Some(binary.as_path())
        );
        assert!(!V2_6_0::check(&paths).expect("check after migration"));

        cleanup(&root).expect("cleanup");
    }
}
