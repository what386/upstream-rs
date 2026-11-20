use anyhow::{Context, Result, bail};
use rusqlite::Connection;

mod api;
mod executables;
mod mapping;
mod packages;
mod patterns;
mod settings;

pub use api::PackageDatabase;
pub use settings::PackageSettings;

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 14;
pub const MIN_PACKAGE_DB_SCHEMA_VERSION: u32 = 11;

const SCHEMA_SQL: &str = include_str!("schema.sql");
const MIGRATION_011_TO_012: &str = include_str!("migrations/011-to-012.sql");

const MIGRATION_013_TO_014: &str = include_str!("migrations/013-to-014.sql");

const MIGRATION_012_TO_013: &str = include_str!("migrations/012-to-013.sql");

fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("Failed to enable package database foreign keys")?;

    let current_version = schema_version(conn)?;
    if current_version == 0 {
        let has_tables: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
            [], |row| row.get(0),
        )?;

        if has_tables {
            bail!(
                "Unversioned package database is not empty. Upgrade this installation with Upstream v2.24.1 before using this version."
            );
        }

        conn.execute_batch(SCHEMA_SQL)
            .context("Failed to initialize package database schema")?;
        record_schema_version(conn)?;
        return Ok(());
    }

    migrate_schema(conn, current_version)?;
    conn.execute_batch(SCHEMA_SQL)
        .context("Failed to initialize package database schema")?;
    Ok(())
}

fn migrate_schema(conn: &Connection, mut current_version: u32) -> Result<()> {
    if current_version < MIN_PACKAGE_DB_SCHEMA_VERSION {
        bail!(
            "Package database schema {current_version} is no longer supported. Upgrade this installation with Upstream v2.24.1 before using this version."
        );
    }

    if current_version > PACKAGE_DB_SCHEMA_VERSION {
        bail!(
            "Unsupported package database schema version {}. Expected version {} or earlier.",
            current_version,
            PACKAGE_DB_SCHEMA_VERSION
        );
    }

    while current_version < PACKAGE_DB_SCHEMA_VERSION {
        let (sql, next_version) = match current_version {
            11 => (MIGRATION_011_TO_012, 12),
            12 => (MIGRATION_012_TO_013, 13),
            13 => (MIGRATION_013_TO_014, 14),
            version => bail!(
                "Unsupported package database schema version {}. Expected version {} or earlier.",
                version,
                PACKAGE_DB_SCHEMA_VERSION
            ),
        };

        conn.execute_batch(sql).with_context(|| {
            format!(
                "Failed to migrate package database schema from version {current_version} to {next_version}"
            )
        })?;
        current_version = next_version;
    }

    Ok(())
}

fn record_schema_version(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "user_version", PACKAGE_DB_SCHEMA_VERSION)
        .context("Failed to record package database schema version")
}

fn schema_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .context("Failed to read package database schema version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v13_provider_constraint_without_losing_relations() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&SCHEMA_SQL.replace(", 'CratesIo'", ""))
            .unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA user_version = 13;
            INSERT INTO packages (id, repo_slug, filetype, version_major, version_minor, version_patch, version_is_prerelease, channel, provider, install_type, is_pinned, last_upgraded)
            VALUES ('github:owner/tool', 'owner/tool', 'Binary', 1, 0, 0, 0, 'Stable', 'Github', 'Build', 0, '2026-01-01T00:00:00Z');
            INSERT INTO package_executables VALUES ('github:owner/tool', '/tools/tool', 'tool');
            INSERT INTO patterns VALUES ('github:owner/tool', 'match', 0, 'linux');").unwrap();
        initialize(&conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 14);
        assert!(
            !conn
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );

        assert_eq!(
            conn.query_row("SELECT count(*) FROM package_executables", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            1
        );

        assert_eq!(
            conn.query_row("SELECT count(*) FROM patterns", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            1
        );

        conn.execute("UPDATE packages SET provider = 'CratesIo'", [])
            .unwrap();
    }

    #[test]
    fn migrates_v11_provider_constraint_and_preserves_package_relations() {
        let conn = Connection::open_in_memory().unwrap();
        let legacy_schema = SCHEMA_SQL
            .replace("\r\n", "\n")
            .replace(", 'AppImageHub'", "")
            .replace("    desktop_launch_options TEXT,\n", "");

        assert!(!legacy_schema.contains("AppImageHub"));
        assert!(!legacy_schema.contains("desktop_launch_options"));
        conn.execute_batch(&legacy_schema).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA user_version = 11;
             INSERT INTO packages (id, repo_slug, filetype, version_major, version_minor,
                 version_patch, version_is_prerelease, channel, provider, install_type,
                 is_pinned, last_upgraded)
             VALUES ('github:owner/tool', 'owner/tool', 'Archive', 1, 0, 0, 0, 'Stable', 'Github', 'Release', 0, '2026-01-01T00:00:00Z');
             INSERT INTO patterns VALUES ('github:owner/tool', 'match', 0, 'linux');
             INSERT INTO path_entries VALUES ('github:owner/tool', '/tools', 0);
             INSERT INTO package_settings VALUES ('github:owner/tool', 'Checksum');
             INSERT INTO package_executables VALUES ('github:owner/tool', '/tools/tool', 'tool');",
        ).unwrap();
        initialize(&conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), PACKAGE_DB_SCHEMA_VERSION);
        for table in [
            "packages",
            "patterns",
            "path_entries",
            "package_settings",
            "package_executables",
        ] {
            let count: u32 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();

            assert_eq!(count, 1, "{table}");
        }

        assert!(
            !conn
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );

        conn.execute_batch(
            "INSERT INTO packages (id, repo_slug, filetype, version_major, version_minor,
                 version_patch, version_is_prerelease, channel, provider, install_type,
                 is_pinned, last_upgraded)
             VALUES ('appimagehub:2259793', '2259793', 'AppImage', 1, 0, 0, 0, 'Stable', 'AppImageHub', 'Release', 0, '2026-01-01T00:00:00Z');",
        ).unwrap();
        // Foreign-key enforcement is restored after rebuilding the table.
        assert!(
            conn.execute("INSERT INTO path_entries VALUES ('missing', '/bad', 1)", [])
                .is_err()
        );

        initialize(&conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), PACKAGE_DB_SCHEMA_VERSION);
    }
}

#[cfg(test)]
mod baseline_tests {
    use super::*;

    #[test]
    fn retired_and_future_schemas_are_rejected_without_changes() {
        for version in (1..MIN_PACKAGE_DB_SCHEMA_VERSION).chain([PACKAGE_DB_SCHEMA_VERSION + 1]) {
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch(
                "CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES ('keep');",
            )
            .unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
            assert!(initialize(&conn).is_err());
            assert_eq!(schema_version(&conn).unwrap(), version);
            assert_eq!(
                conn.query_row("SELECT value FROM preserved", [], |r| r.get::<_, String>(0))
                    .unwrap(),
                "keep"
            );

            assert!(
                !conn
                    .prepare("SELECT name FROM sqlite_master WHERE name='packages'")
                    .unwrap()
                    .exists([])
                    .unwrap()
            );
        }
    }

    #[test]
    fn only_empty_unversioned_databases_are_initialized() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), PACKAGE_DB_SCHEMA_VERSION);
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE packages(value TEXT);")
            .unwrap();
        assert!(initialize(&conn).is_err());
        assert_eq!(schema_version(&conn).unwrap(), 0);
    }
}
