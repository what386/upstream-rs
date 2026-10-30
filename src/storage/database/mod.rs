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

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 12;

const SCHEMA_SQL: &str = include_str!("schema.sql");
const MIGRATION_001_TO_002: &str = include_str!("migrations/001-to-002.sql");
const MIGRATION_002_TO_003: &str = include_str!("migrations/002-to-003.sql");
const MIGRATION_003_TO_004: &str = include_str!("migrations/003-to-004.sql");
const MIGRATION_004_TO_005: &str = include_str!("migrations/004-to-005.sql");
const MIGRATION_005_TO_006: &str = include_str!("migrations/005-to-006.sql");
const MIGRATION_006_TO_007: &str = include_str!("migrations/006-to-007.sql");
const MIGRATION_007_TO_008: &str = include_str!("migrations/007-to-008.sql");
const MIGRATION_008_TO_009: &str = include_str!("migrations/008-to-009.sql");
const MIGRATION_009_TO_010: &str = include_str!("migrations/009-to-010.sql");
const MIGRATION_010_TO_011: &str = include_str!("migrations/010-to-011.sql");
const MIGRATION_011_TO_012: &str = include_str!("migrations/011-to-012.sql");

fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("Failed to enable package database foreign keys")?;

    let current_version = schema_version(conn)?;
    if current_version == 0 {
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
    if current_version > PACKAGE_DB_SCHEMA_VERSION {
        bail!(
            "Unsupported package database schema version {}. Expected version {} or earlier.",
            current_version,
            PACKAGE_DB_SCHEMA_VERSION
        );
    }

    while current_version < PACKAGE_DB_SCHEMA_VERSION {
        let (sql, next_version) = match current_version {
            1 => (MIGRATION_001_TO_002, 2),
            2 => (MIGRATION_002_TO_003, 3),
            3 => (MIGRATION_003_TO_004, 4),
            4 => (MIGRATION_004_TO_005, 5),
            5 => (MIGRATION_005_TO_006, 6),
            6 => (MIGRATION_006_TO_007, 7),
            7 => (MIGRATION_007_TO_008, 8),
            8 => (MIGRATION_008_TO_009, 9),
            9 => (MIGRATION_009_TO_010, 10),
            10 => (MIGRATION_010_TO_011, 11),
            11 => (MIGRATION_011_TO_012, 12),
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
    fn migrates_v11_provider_constraint_and_preserves_package_relations() {
        let conn = Connection::open_in_memory().unwrap();
        let legacy_schema = SCHEMA_SQL.replace(", 'AppImageHub'", "");
        assert!(!legacy_schema.contains("AppImageHub"));
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
