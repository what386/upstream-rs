use anyhow::{Context, Result, anyhow};
use rusqlite::Connection;

use crate::routines::doctor::checks::legacy;
use crate::routines::migrate::MigrationReport;
use crate::services::integration::SymlinkManager;
use crate::storage::database::PackageDatabase;
use crate::utils::static_paths::UpstreamPaths;

const PACKAGE_DB_SCHEMA_VERSION_WITH_TAG_TEMPLATE: u32 = 2;

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    let database_exists = paths.config.packages_database_file.exists();
    if database_exists {
        migrate_package_database_schema(paths)?;
    }

    let mut storage = PackageDatabase::open(&paths.config.packages_database_file)?;
    let packages = if !database_exists && legacy::legacy_package_metadata_exists(paths) {
        let packages = legacy::load_legacy_package_metadata(paths)?;
        storage.replace_all_packages(&packages)?;
        packages
    } else {
        storage.list_packages()?
    };

    if database_exists {
        refresh_symlinks(paths, &packages, report)?;
    }

    Ok(())
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

fn refresh_symlinks(
    paths: &UpstreamPaths,
    packages: &[crate::models::upstream::Package],
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

#[cfg(test)]
mod tests {
    use super::run;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::routines::migrate::MigrationReport;
    use crate::storage::database::PackageDatabase;
    use crate::utils::test_support;
    use rusqlite::Connection;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-v2-6-test", name)
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
    fn migrate_imports_rewritten_package_json_into_database() {
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

        assert_eq!(report.refreshed_symlinks, 0);
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

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_upgrades_package_database_schema() {
        let root = temp_root("package-db-schema");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(
            paths
                .config
                .packages_database_file
                .parent()
                .expect("database parent"),
        )
        .expect("create database parent");
        let conn = Connection::open(&paths.config.packages_database_file).expect("open sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE packages (
                name TEXT PRIMARY KEY NOT NULL,
                repo_slug TEXT NOT NULL,
                filetype TEXT NOT NULL,
                version_major INTEGER NOT NULL,
                version_minor INTEGER NOT NULL,
                version_patch INTEGER NOT NULL,
                version_is_prerelease INTEGER NOT NULL,
                channel TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT,
                install_type TEXT NOT NULL,
                build_branch TEXT,
                build_commit TEXT,
                is_pinned INTEGER NOT NULL,
                icon_path TEXT,
                install_path TEXT,
                exec_path TEXT,
                last_upgraded TEXT NOT NULL
            );
            CREATE TABLE patterns (
                package_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                position INTEGER NOT NULL,
                pattern TEXT NOT NULL,
                PRIMARY KEY (package_name, kind, position),
                FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE
            );
            CREATE INDEX idx_patterns_package_kind_position
                ON patterns(package_name, kind, position);
            PRAGMA user_version = 1;
            INSERT INTO packages (
                name,
                repo_slug,
                filetype,
                version_major,
                version_minor,
                version_patch,
                version_is_prerelease,
                channel,
                provider,
                base_url,
                install_type,
                build_branch,
                build_commit,
                is_pinned,
                icon_path,
                install_path,
                exec_path,
                last_upgraded
            ) VALUES (
                'codex',
                'openai/codex',
                'Archive',
                0,
                142,
                0,
                0,
                'Stable',
                'Github',
                NULL,
                'Release',
                NULL,
                NULL,
                0,
                NULL,
                NULL,
                NULL,
                '2026-06-26T00:00:00Z'
            );
            "#,
        )
        .expect("create v1 package database");
        drop(conn);
        let mut report = MigrationReport::default();

        run(&paths, &mut report).expect("migrate package database schema");

        assert_eq!(report.skipped_symlinks, 1);
        let migrated_storage = PackageDatabase::open(&paths.config.packages_database_file)
            .expect("open migrated package database");
        assert_eq!(
            migrated_storage.schema_version().expect("schema version"),
            crate::storage::database::PACKAGE_DB_SCHEMA_VERSION
        );
        let migrated_package = migrated_storage
            .get_package("codex")
            .expect("load migrated package")
            .expect("migrated package");
        assert!(migrated_package.version_tag_template.is_none());

        cleanup(&root).expect("cleanup");
    }
}
