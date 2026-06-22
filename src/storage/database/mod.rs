use anyhow::{Context, Result};
use rusqlite::Connection;

mod mapping;
mod packages;
mod patterns;

pub use packages::PackageDatabase;

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 1;

const SCHEMA_SQL: &str = include_str!("schema.sql");

fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("Failed to enable package database foreign keys")?;
    conn.execute_batch(SCHEMA_SQL)
        .context("Failed to initialize package database schema")?;

    conn.pragma_update(None, "user_version", PACKAGE_DB_SCHEMA_VERSION)
        .context("Failed to record package database schema version")?;
    Ok(())
}

fn schema_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .context("Failed to read package database schema version")
}
