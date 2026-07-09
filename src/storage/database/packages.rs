use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::models::upstream::Package;

use super::mapping::{
    PACKAGE_COLUMNS, bool_to_db, enum_to_db, optional_path_to_db, row_to_package,
};
use super::patterns::{load_patterns, load_patterns_for_packages, replace_patterns};

#[derive(Debug)]
pub struct PackageConnection {
    conn: Connection,
}

impl PackageConnection {
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

    #[cfg(test)]
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

        let mut packages = stmt
            .query_map([], row_to_package)
            .context("Failed to list packages")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode package rows")?;
        drop(stmt);

        load_patterns_for_packages(&self.conn, &mut packages)?;
        Ok(packages)
    }

    pub fn list_path_entries(&self) -> Result<Vec<PathBuf>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM path_entries ORDER BY position")
            .context("Failed to prepare PATH entry list query")?;

        stmt.query_map([], |row| row.get::<_, String>(0).map(PathBuf::from))
            .context("Failed to list PATH entries")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to decode PATH entry rows")
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

    pub fn add_path_entry(&mut self, package_name: &str, path: &Path) -> Result<bool> {
        let path = path_to_db(path)?;
        let tx = self
            .conn
            .transaction()
            .context("Failed to start PATH entry insert transaction")?;

        let existing_path = tx
            .query_row(
                "SELECT path FROM path_entries WHERE package_name = ?1",
                [package_name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .with_context(|| format!("Failed to check PATH entry for '{}'", package_name))?;
        if existing_path.as_deref() == Some(path.as_str()) {
            tx.commit()
                .context("Failed to commit unchanged PATH entry transaction")?;
            return Ok(false);
        }
        if existing_path.is_some() {
            tx.execute(
                "UPDATE path_entries SET path = ?1 WHERE package_name = ?2",
                params![path, package_name],
            )
            .with_context(|| format!("Failed to update PATH entry for '{}'", package_name))?;
            tx.commit()
                .with_context(|| format!("Failed to commit PATH entry '{}'", package_name))?;
            return Ok(true);
        }

        let position = tx
            .query_row(
                "SELECT COALESCE(MIN(position), 0) - 1 FROM path_entries",
                [],
                |row| row.get::<_, i64>(0),
            )
            .context("Failed to determine PATH entry position")?;
        tx.execute(
            "INSERT INTO path_entries (package_name, path, position) VALUES (?1, ?2, ?3)",
            params![package_name, path, position],
        )
        .with_context(|| format!("Failed to add PATH entry for '{}'", package_name))?;
        tx.commit()
            .with_context(|| format!("Failed to commit PATH entry '{}'", package_name))?;
        Ok(true)
    }

    pub fn remove_path_entry(&mut self, package_name: &str) -> Result<bool> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start PATH entry removal transaction")?;

        let affected = tx
            .execute(
                "DELETE FROM path_entries WHERE package_name = ?1",
                [package_name],
            )
            .with_context(|| format!("Failed to remove PATH entry for '{}'", package_name))?;
        tx.commit()
            .with_context(|| format!("Failed to commit PATH entry removal '{}'", package_name))?;
        Ok(affected > 0)
    }

    pub fn replace_all_path_entries(&mut self, entries: &[(String, PathBuf)]) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start PATH entry replacement transaction")?;
        tx.execute("DELETE FROM path_entries", [])
            .context("Failed to clear PATH entries")?;
        for (position, (package_name, path)) in entries.iter().enumerate() {
            let path = path_to_db(path)?;
            tx.execute(
                "INSERT INTO path_entries (package_name, path, position) VALUES (?1, ?2, ?3)",
                params![package_name, path, position as i64],
            )
            .context("Failed to insert PATH entry")?;
        }
        tx.commit()
            .context("Failed to commit PATH entry replacement transaction")
    }

    pub fn replace_all_packages(&mut self, packages: &[Package]) -> Result<()> {
        let tx = self
            .conn
            .transaction()
            .context("Failed to start package replacement transaction")?;
        tx.execute("DELETE FROM packages", [])
            .context("Failed to clear package database")?;
        for package in packages {
            write_package(&tx, package)?;
        }
        tx.commit()
            .context("Failed to commit package replacement transaction")
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
            tx.execute(
                "UPDATE packages SET name = ?1 WHERE name = ?2",
                params![package.name, name],
            )
            .with_context(|| {
                format!("Failed to rename package '{}' to '{}'", name, package.name)
            })?;
        }
        write_package(&tx, &package)?;
        tx.commit()
            .with_context(|| format!("Failed to commit package '{}'", package.name))
    }

    fn initialize(&mut self) -> Result<()> {
        super::initialize(&self.conn)
    }
}

fn path_to_db(path: &Path) -> Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Path '{}' is not valid UTF-8", path.display()))
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
            version_tag_template,
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
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
        )
        ON CONFLICT(name) DO UPDATE SET
            repo_slug = excluded.repo_slug,
            filetype = excluded.filetype,
            version_major = excluded.version_major,
            version_minor = excluded.version_minor,
            version_patch = excluded.version_patch,
            version_is_prerelease = excluded.version_is_prerelease,
            version_tag_template = excluded.version_tag_template,
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
            package.version_tag_template,
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
    use super::PackageConnection;
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
    use rusqlite::Connection;
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
        package.version_tag_template = Some("rust-v{}-beta.4".to_string());
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
        let db = PackageConnection::open_in_memory().expect("open db");

        assert_eq!(
            db.schema_version().expect("schema version"),
            PACKAGE_DB_SCHEMA_VERSION
        );
        assert!(!db.package_exists("missing").expect("exists check"));
    }

    #[test]
    fn open_migrates_schema_v1_in_place() {
        let conn = Connection::open_in_memory().expect("open sqlite");
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
                PRIMARY KEY (package_name, kind, position)
            );
            PRAGMA user_version = 1;
            "#,
        )
        .expect("create v1 schema");

        let db = PackageConnection::from_connection(conn).expect("migrate v1 schema");

        assert_eq!(
            db.schema_version().expect("schema version"),
            PACKAGE_DB_SCHEMA_VERSION
        );
        assert!(!db.package_exists("missing").expect("exists check"));
    }

    #[test]
    fn open_migrates_schema_v4_path_entries_in_place() {
        let conn = Connection::open_in_memory().expect("open sqlite");
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
                version_tag_template TEXT,
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
                kind TEXT NOT NULL CHECK (kind IN ('match', 'exclude')),
                position INTEGER NOT NULL CHECK (position >= 0),
                pattern TEXT NOT NULL,
                PRIMARY KEY (package_name, kind, position),
                FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
            );
            CREATE TABLE path_entries (
                package_name TEXT PRIMARY KEY NOT NULL,
                path TEXT NOT NULL,
                position INTEGER NOT NULL CHECK (position >= 0),
                FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
            );
            INSERT INTO packages (
                name, repo_slug, filetype, version_major, version_minor, version_patch,
                version_is_prerelease, version_tag_template, channel, provider, base_url,
                install_type, build_branch, build_commit, is_pinned, icon_path, install_path,
                exec_path, last_upgraded
            ) VALUES
                ('first', 'owner/first', 'Archive', 1, 0, 0, 0, NULL, 'Stable', 'Github', NULL,
                 'Release', NULL, NULL, 0, NULL, '/packages/first', '/packages/first/bin/first',
                 '2026-06-21T12:30:00Z'),
                ('second', 'owner/second', 'Archive', 1, 0, 0, 0, NULL, 'Stable', 'Github', NULL,
                 'Release', NULL, NULL, 0, NULL, '/packages/second', '/packages/second/bin/second',
                 '2026-06-21T12:31:00Z');
            INSERT INTO path_entries (package_name, path, position) VALUES
                ('first', '/first/bin', 0),
                ('second', '/second/bin', 1);
            PRAGMA user_version = 4;
            "#,
        )
        .expect("create v4 schema");

        let mut db = PackageConnection::from_connection(conn).expect("migrate v4 schema");

        assert_eq!(
            db.schema_version().expect("schema version"),
            PACKAGE_DB_SCHEMA_VERSION
        );
        assert_eq!(
            db.list_path_entries().expect("list migrated path entries"),
            vec![PathBuf::from("/first/bin"), PathBuf::from("/second/bin")]
        );

        db.upsert_package(&test_package("third"))
            .expect("seed third package");
        db.add_path_entry("third", &PathBuf::from("/third/bin"))
            .expect("add sparse path entry");
        assert_eq!(
            db.list_path_entries().expect("list sparse path entries"),
            vec![
                PathBuf::from("/third/bin"),
                PathBuf::from("/first/bin"),
                PathBuf::from("/second/bin"),
            ]
        );
    }

    #[test]
    fn upsert_and_get_package_round_trips_all_fields() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
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
        assert_eq!(stored.version_tag_template, package.version_tag_template);
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
    fn path_entries_are_ordered_by_newest_entry_first() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
        let first = PathBuf::from("/first/bin");
        let second = PathBuf::from("/second/bin");
        let third = PathBuf::from("/third/bin");

        db.upsert_package(&test_package("first"))
            .expect("seed first package");
        db.upsert_package(&test_package("second"))
            .expect("seed second package");
        db.upsert_package(&test_package("third"))
            .expect("seed third package");
        assert!(db.add_path_entry("first", &first).expect("add first"));
        assert!(db.add_path_entry("second", &second).expect("add second"));
        assert!(db.add_path_entry("third", &third).expect("add third"));
        assert!(!db.add_path_entry("second", &second).expect("dedupe second"));

        assert_eq!(
            db.list_path_entries().expect("list path entries"),
            vec![third.clone(), second.clone(), first.clone()]
        );

        assert!(db.remove_path_entry("second").expect("remove second"));
        assert!(
            !db.remove_path_entry("second")
                .expect("remove missing second")
        );
        assert_eq!(
            db.list_path_entries().expect("list path entries"),
            vec![third, first]
        );
    }

    #[test]
    fn replace_all_path_entries_preserves_supplied_order() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
        db.upsert_package(&test_package("first"))
            .expect("seed first package");
        db.upsert_package(&test_package("second"))
            .expect("seed second package");

        db.replace_all_path_entries(&[
            ("second".to_string(), PathBuf::from("/second/bin")),
            ("first".to_string(), PathBuf::from("/first/bin")),
        ])
        .expect("replace path entries");

        assert_eq!(
            db.list_path_entries().expect("list path entries"),
            vec![PathBuf::from("/second/bin"), PathBuf::from("/first/bin")]
        );
    }

    #[test]
    fn upsert_replaces_package_and_patterns() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
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
        let mut db = PackageConnection::open_in_memory().expect("open db");
        db.upsert_package(&test_package("zulu"))
            .expect("upsert zulu");
        let mut alpha = test_package("alpha");
        alpha.match_pattern = PatternTable::from_patterns(["alpha", "linux"]);
        alpha.exclude_pattern = PatternTable::from_patterns(["debug"]);
        db.upsert_package(&alpha).expect("upsert alpha");

        let packages = db.list_packages().expect("list packages");
        let names = packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["alpha", "zulu"]);
        assert_eq!(
            packages[0].match_pattern.as_slice(),
            &["alpha".to_string(), "linux".to_string()]
        );
        assert_eq!(
            packages[0].exclude_pattern.as_slice(),
            &["debug".to_string()]
        );
    }

    #[test]
    fn remove_package_deletes_package_and_patterns() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
        db.upsert_package(&test_package("tool"))
            .expect("upsert package");

        assert!(db.remove_package("tool").expect("remove package"));
        assert!(!db.remove_package("tool").expect("remove missing package"));
        assert!(db.get_package("tool").expect("load missing").is_none());
        assert!(!db.package_exists("tool").expect("exists check"));
    }

    #[test]
    fn update_package_mutates_one_package() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
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
        let mut db = PackageConnection::open_in_memory().expect("open db");
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

    #[test]
    fn rename_package_cascades_path_entries() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
        let path = PathBuf::from("/old/bin");
        db.upsert_package(&test_package("old"))
            .expect("upsert package");
        db.add_path_entry("old", &path).expect("add path entry");

        db.update_package("old", |package| {
            package.name = "new".to_string();
            Ok(())
        })
        .expect("rename package");

        assert!(!db.remove_path_entry("old").expect("remove old entry"));
        assert!(db.remove_path_entry("new").expect("remove new entry"));
        assert!(
            db.list_path_entries()
                .expect("list path entries")
                .is_empty()
        );
    }

    #[test]
    fn replace_all_packages_replaces_previous_rows() {
        let mut db = PackageConnection::open_in_memory().expect("open db");
        db.upsert_package(&test_package("old"))
            .expect("upsert old package");

        db.replace_all_packages(&[test_package("new")])
            .expect("replace all packages");

        assert!(db.get_package("old").expect("load old").is_none());
        assert!(db.get_package("new").expect("load new").is_some());
    }
}
