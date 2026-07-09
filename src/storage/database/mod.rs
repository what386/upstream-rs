use anyhow::{Context, Result, bail};
use rusqlite::Connection;

mod api;
mod mapping;
mod packages;
mod patterns;

pub use api::PackageDatabase;

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 3;

const SCHEMA_SQL: &str = include_str!("schema.sql");

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
        match current_version {
            1 => {
                conn.execute_batch(
                    "
                    BEGIN;
                    ALTER TABLE packages ADD COLUMN version_tag_template TEXT;
                    PRAGMA user_version = 2;
                    COMMIT;
                    ",
                )
                .context("Failed to migrate package database schema from version 1 to 2")?;
                current_version = 2;
            }
            2 => {
                conn.execute_batch(
                    "
                    BEGIN;
                    CREATE TABLE IF NOT EXISTS path_entries (
                        path TEXT PRIMARY KEY NOT NULL,
                        position INTEGER NOT NULL CHECK (position >= 0)
                    );
                    PRAGMA user_version = 3;
                    COMMIT;
                    ",
                )
                .context("Failed to migrate package database schema from version 2 to 3")?;
                current_version = 3;
            }
            version => bail!(
                "Unsupported package database schema version {}. Expected version {} or earlier.",
                version,
                PACKAGE_DB_SCHEMA_VERSION
            ),
        }
    }

    if current_version == PACKAGE_DB_SCHEMA_VERSION {
        return Ok(());
    }

    unreachable!("schema migration loop should finish at the current schema version")
}

fn record_schema_version(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "user_version", PACKAGE_DB_SCHEMA_VERSION)
        .context("Failed to record package database schema version")
}

fn schema_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .context("Failed to read package database schema version")
}
