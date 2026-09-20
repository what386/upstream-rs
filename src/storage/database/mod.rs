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

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 11;

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
