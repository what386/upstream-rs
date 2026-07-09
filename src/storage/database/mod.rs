use anyhow::{Context, Result, bail};
use rusqlite::Connection;

mod api;
mod mapping;
mod packages;
mod patterns;

pub use api::PackageDatabase;

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 5;

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
            3 => {
                conn.execute_batch(
                    "
                    BEGIN;
                    CREATE TABLE patterns_new (
                        package_name TEXT NOT NULL,
                        kind TEXT NOT NULL CHECK (kind IN ('match', 'exclude')),
                        position INTEGER NOT NULL CHECK (position >= 0),
                        pattern TEXT NOT NULL,
                        PRIMARY KEY (package_name, kind, position),
                        FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
                    );
                    INSERT INTO patterns_new (package_name, kind, position, pattern)
                        SELECT patterns.package_name, patterns.kind, patterns.position, patterns.pattern
                        FROM patterns
                        INNER JOIN packages ON packages.name = patterns.package_name;
                    DROP TABLE patterns;
                    ALTER TABLE patterns_new RENAME TO patterns;
                    CREATE INDEX IF NOT EXISTS idx_patterns_package_kind_position
                        ON patterns(package_name, kind, position);

                    DROP TABLE IF EXISTS path_entries;
                    CREATE TABLE path_entries (
                        package_name TEXT PRIMARY KEY NOT NULL,
                        path TEXT NOT NULL,
                        position INTEGER NOT NULL CHECK (position >= 0),
                        FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
                    );
                    PRAGMA user_version = 4;
                    COMMIT;
                    ",
                )
                .context("Failed to migrate package database schema from version 3 to 4")?;
                current_version = 4;
            }
            4 => {
                conn.execute_batch(
                    "
                    BEGIN;
                    CREATE TABLE path_entries_new (
                        package_name TEXT PRIMARY KEY NOT NULL,
                        path TEXT NOT NULL,
                        position INTEGER NOT NULL,
                        FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
                    );
                    INSERT INTO path_entries_new (package_name, path, position)
                        SELECT path_entries.package_name, path_entries.path, path_entries.position
                        FROM path_entries
                        INNER JOIN packages ON packages.name = path_entries.package_name
                        ORDER BY path_entries.position ASC;
                    DROP TABLE path_entries;
                    ALTER TABLE path_entries_new RENAME TO path_entries;
                    CREATE INDEX IF NOT EXISTS idx_path_entries_position
                        ON path_entries(position);
                    PRAGMA user_version = 5;
                    COMMIT;
                    ",
                )
                .context("Failed to migrate package database schema from version 4 to 5")?;
                current_version = 5;
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
