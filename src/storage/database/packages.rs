use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::models::upstream::Package;

use super::mapping::{
    PACKAGE_COLUMNS, bool_to_db, enum_to_db, optional_path_to_db, row_to_package,
};
use super::patterns::{load_patterns, replace_patterns};

#[derive(Debug)]
pub struct PackageDatabase {
    conn: Connection,
}

impl PackageDatabase {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create package database directory '{}'",
                    parent.display()
                )
            })?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open package database '{}'", path.display()))?;
        Self::from_connection(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(
            Connection::open_in_memory().context("Failed to open package database in memory")?,
        )
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        let mut db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    pub fn schema_version(&self) -> Result<u32> {
        super::schema_version(&self.conn)
    }

    pub fn package_exists(&self, name: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM packages WHERE name = ?1)",
                [name],
                |row| row.get::<_, bool>(0),
            )
            .with_context(|| format!("Failed to check package '{}'", name))
    }

    pub fn get_package(&self, name: &str) -> Result<Option<Package>> {
        let package = self
            .conn
            .query_row(&select_package_by_name_query(), [name], row_to_package)
            .optional()
            .with_context(|| format!("Failed to load package '{}'", name))?;

        match package {
            Some(mut package) => {
                load_patterns(&self.conn, &mut package)?;
                Ok(Some(package))
            }
            None => Ok(None),
        }
    }

    pub fn list_packages(&self) -> Result<Vec<Package>> {
        let mut stmt = self
            .conn
            .prepare(&list_packages_query())
            .context("Failed to prepare package list query")?;

        let packages = stmt
            .query_map([], row_to_package)
            .context("Failed to list packages")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode package rows")?;
        drop(stmt);

        packages
            .into_iter()
            .map(|mut package| {
                load_patterns(&self.conn, &mut package)?;
                Ok(package)
            })
            .collect()
    }

    pub fn upsert_package(&mut self, package: &Package) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start package upsert transaction")?;
        write_package(&tx, package)?;
        tx.commit()
            .with_context(|| format!("Failed to commit package '{}'", package.name))
    }

    pub fn remove_package(&mut self, name: &str) -> Result<bool> {
        let affected = self
            .conn
            .execute("DELETE FROM packages WHERE name = ?1", [name])
            .with_context(|| format!("Failed to remove package '{}'", name))?;
        Ok(affected > 0)
    }

    pub fn update_package<F>(&mut self, name: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut Package) -> Result<()>,
    {
        let mut package = self
            .get_package(name)?
            .ok_or_else(|| anyhow!("Package '{}' not found", name))?;
        update(&mut package)?;

        let tx = self
            .conn
            .transaction()
            .context("Failed to start package update transaction")?;
        if package.name != name {
            tx.execute("DELETE FROM packages WHERE name = ?1", [name])
                .with_context(|| format!("Failed to remove renamed package '{}'", name))?;
        }
        write_package(&tx, &package)?;
        tx.commit()
            .with_context(|| format!("Failed to commit package '{}'", package.name))
    }

    fn initialize(&mut self) -> Result<()> {
        super::initialize(&self.conn)
    }
}

fn select_package_by_name_query() -> String {
    format!("SELECT {PACKAGE_COLUMNS} FROM packages WHERE name = ?1")
}

fn list_packages_query() -> String {
    format!("SELECT {PACKAGE_COLUMNS} FROM packages ORDER BY lower(name), name")
}

fn write_package(tx: &Transaction<'_>, package: &Package) -> Result<()> {
    tx.execute(
        "INSERT INTO packages (
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
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
        )
        ON CONFLICT(name) DO UPDATE SET
            repo_slug = excluded.repo_slug,
            filetype = excluded.filetype,
            version_major = excluded.version_major,
            version_minor = excluded.version_minor,
            version_patch = excluded.version_patch,
            version_is_prerelease = excluded.version_is_prerelease,
            channel = excluded.channel,
            provider = excluded.provider,
            base_url = excluded.base_url,
            install_type = excluded.install_type,
            build_branch = excluded.build_branch,
            build_commit = excluded.build_commit,
            is_pinned = excluded.is_pinned,
            icon_path = excluded.icon_path,
            install_path = excluded.install_path,
            exec_path = excluded.exec_path,
            last_upgraded = excluded.last_upgraded",
        params![
            package.name,
            package.repo_slug,
            enum_to_db(&package.filetype)?,
            package.version.major,
            package.version.minor,
            package.version.patch,
            bool_to_db(package.version.is_prerelease),
            enum_to_db(&package.channel)?,
            enum_to_db(&package.provider)?,
            package.base_url,
            enum_to_db(&package.install_type)?,
            package.build_branch,
            package.build_commit,
            bool_to_db(package.is_pinned),
            optional_path_to_db(&package.icon_path)?,
            optional_path_to_db(&package.install_path)?,
            optional_path_to_db(&package.exec_path)?,
            package.last_upgraded.to_rfc3339(),
        ],
    )
    .with_context(|| format!("Failed to write package '{}'", package.name))?;

    replace_patterns(tx, package)
}

#[cfg(test)]
mod tests {
    use super::PackageDatabase;
    use crate::models::{
        common::{
            Version,
            enums::{Channel, Filetype, Provider},
        },
        upstream::{InstallType, Package},
    };
    use crate::providers::pattern_matcher::PatternTable;
    use crate::storage::database::PACKAGE_DB_SCHEMA_VERSION;
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;

    fn test_package(name: &str) -> Package {
        let mut package = Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            Channel::Preview,
            Provider::Github,
            Some("https://api.github.com".to_string()),
        );
        package.version = Version::new(1, 2, 3, true);
        package.install_type = InstallType::Build;
        package.build_branch = Some("main".to_string());
        package.build_commit = Some("abcdef".to_string());
        package.is_pinned = true;
        package.match_pattern = PatternTable::from_patterns(["linux", "x86_64"]);
        package.exclude_pattern = PatternTable::from_patterns(["debug", "symbols"]);
        package.icon_path = Some(PathBuf::from("/icons/tool.png"));
        package.install_path = Some(PathBuf::from("/packages/tool"));
        package.exec_path = Some(PathBuf::from("/packages/tool/bin/tool"));
        package.last_upgraded = Utc
            .with_ymd_and_hms(2026, 6, 21, 12, 30, 0)
            .single()
            .expect("valid timestamp");
        package
    }

    #[test]
    fn open_in_memory_initializes_schema() {
        let db = PackageDatabase::open_in_memory().expect("open db");

        assert_eq!(
            db.schema_version().expect("schema version"),
            PACKAGE_DB_SCHEMA_VERSION
        );
        assert!(!db.package_exists("missing").expect("exists check"));
    }

    #[test]
    fn upsert_and_get_package_round_trips_all_fields() {
        let mut db = PackageDatabase::open_in_memory().expect("open db");
        let package = test_package("tool");

        db.upsert_package(&package).expect("upsert package");
        let stored = db
            .get_package("tool")
            .expect("load package")
            .expect("package exists");

        assert_eq!(stored.name, package.name);
        assert_eq!(stored.repo_slug, package.repo_slug);
        assert_eq!(stored.filetype, package.filetype);
        assert_eq!(stored.version, package.version);
        assert_eq!(stored.channel, package.channel);
        assert_eq!(stored.provider, package.provider);
        assert_eq!(stored.base_url, package.base_url);
        assert_eq!(stored.install_type, package.install_type);
        assert_eq!(stored.build_branch, package.build_branch);
        assert_eq!(stored.build_commit, package.build_commit);
        assert_eq!(stored.is_pinned, package.is_pinned);
        assert_eq!(
            stored.match_pattern.as_slice(),
            package.match_pattern.as_slice()
        );
        assert_eq!(
            stored.exclude_pattern.as_slice(),
            package.exclude_pattern.as_slice()
        );
        assert_eq!(stored.icon_path, package.icon_path);
        assert_eq!(stored.install_path, package.install_path);
        assert_eq!(stored.exec_path, package.exec_path);
        assert_eq!(stored.last_upgraded, package.last_upgraded);
    }

    #[test]
    fn upsert_replaces_package_and_patterns() {
        let mut db = PackageDatabase::open_in_memory().expect("open db");
        let mut package = test_package("tool");
        db.upsert_package(&package).expect("upsert package");

        package.version = Version::new(2, 0, 0, false);
        package.match_pattern = PatternTable::from_patterns(["aarch64"]);
        package.exclude_pattern = PatternTable::empty();
        db.upsert_package(&package).expect("replace package");

        let stored = db
            .get_package("tool")
            .expect("load package")
            .expect("package exists");
        assert_eq!(stored.version, Version::new(2, 0, 0, false));
        assert_eq!(stored.match_pattern.as_slice(), &["aarch64".to_string()]);
        assert!(stored.exclude_pattern.is_empty());
    }

    #[test]
    fn list_packages_is_sorted_by_name() {
        let mut db = PackageDatabase::open_in_memory().expect("open db");
        db.upsert_package(&test_package("zulu"))
            .expect("upsert zulu");
        db.upsert_package(&test_package("alpha"))
            .expect("upsert alpha");

        let names = db
            .list_packages()
            .expect("list packages")
            .into_iter()
            .map(|package| package.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["alpha", "zulu"]);
    }

    #[test]
    fn remove_package_deletes_package_and_patterns() {
        let mut db = PackageDatabase::open_in_memory().expect("open db");
        db.upsert_package(&test_package("tool"))
            .expect("upsert package");

        assert!(db.remove_package("tool").expect("remove package"));
        assert!(!db.remove_package("tool").expect("remove missing package"));
        assert!(db.get_package("tool").expect("load missing").is_none());
        assert!(!db.package_exists("tool").expect("exists check"));
    }

    #[test]
    fn update_package_mutates_one_package() {
        let mut db = PackageDatabase::open_in_memory().expect("open db");
        db.upsert_package(&test_package("tool"))
            .expect("upsert package");

        db.update_package("tool", |package| {
            package.is_pinned = false;
            package.exec_path = Some(PathBuf::from("/new/tool"));
            Ok(())
        })
        .expect("update package");

        let stored = db
            .get_package("tool")
            .expect("load package")
            .expect("package exists");
        assert!(!stored.is_pinned);
        assert_eq!(stored.exec_path, Some(PathBuf::from("/new/tool")));
    }

    #[test]
    fn update_package_supports_rename() {
        let mut db = PackageDatabase::open_in_memory().expect("open db");
        db.upsert_package(&test_package("old"))
            .expect("upsert package");

        db.update_package("old", |package| {
            package.name = "new".to_string();
            Ok(())
        })
        .expect("rename package");

        assert!(db.get_package("old").expect("load old").is_none());
        assert!(db.get_package("new").expect("load new").is_some());
    }
}
