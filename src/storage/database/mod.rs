use anyhow::{Context, Result, bail};
use rusqlite::Connection;

mod api;
mod mapping;
mod packages;
mod patterns;

pub use api::PackageDatabase;

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 2;

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

    validate_schema_version(current_version)?;
    conn.execute_batch(SCHEMA_SQL)
        .context("Failed to initialize package database schema")?;
    Ok(())
}

fn validate_schema_version(current_version: u32) -> Result<()> {
    if current_version == PACKAGE_DB_SCHEMA_VERSION {
        return Ok(());
    }

    bail!(
        "Unsupported package database schema version {}. Expected version {}. Run `upstream doctor --migrate` to update package metadata.",
        current_version,
        PACKAGE_DB_SCHEMA_VERSION
    )
}

fn record_schema_version(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "user_version", PACKAGE_DB_SCHEMA_VERSION)
        .context("Failed to record package database schema version")
}

fn schema_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .context("Failed to read package database schema version")
}
