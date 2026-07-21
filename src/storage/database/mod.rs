use anyhow::{Context, Result, bail};
use rusqlite::Connection;

mod api;
mod mapping;
mod packages;
mod patterns;
mod settings;

pub use api::PackageDatabase;
pub use settings::PackageSettings;

pub const PACKAGE_DB_SCHEMA_VERSION: u32 = 8;

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
            5 => {
                conn.execute_batch(
                    "
                    BEGIN;
                    CREATE TABLE IF NOT EXISTS package_settings (
                        package_name TEXT PRIMARY KEY NOT NULL,
                        trust_mode TEXT CHECK (
                            trust_mode IS NULL OR trust_mode IN (
                                'None',
                                'BestEffort',
                                'Checksum',
                                'Signature',
                                'All'
                            )
                        ),
                        FOREIGN KEY (package_name) REFERENCES packages(name) ON DELETE CASCADE ON UPDATE CASCADE
                    );
                    PRAGMA user_version = 6;
                    COMMIT;
                    ",
                )
                .context("Failed to migrate package database schema from version 5 to 6")?;
                current_version = 6;
            }
            6 => {
                let has_kind = table_has_column(conn, "packages", "version_kind")?;
                let has_value = table_has_column(conn, "packages", "version_value")?;
                let additions = match (has_kind, has_value) {
                    (false, false) => {
                        "
                        ALTER TABLE packages ADD COLUMN version_kind TEXT NOT NULL DEFAULT 'Semver'
                            CHECK (version_kind IN ('Unknown', 'Semver', 'Datetime'));
                        ALTER TABLE packages ADD COLUMN version_value TEXT;
                    "
                    }
                    (false, true) => {
                        "
                        ALTER TABLE packages ADD COLUMN version_kind TEXT NOT NULL DEFAULT 'Semver'
                            CHECK (version_kind IN ('Unknown', 'Semver', 'Datetime'));
                    "
                    }
                    (true, false) => "ALTER TABLE packages ADD COLUMN version_value TEXT;",
                    (true, true) => "",
                };
                conn.execute_batch(&format!(
                    "BEGIN; {additions} PRAGMA user_version = 7; COMMIT;"
                ))
                .context("Failed to migrate package database schema from version 6 to 7")?;
                current_version = 7;
            }
            7 => {
                let has_tag = table_has_column(conn, "packages", "release_tag")?;
                let has_published = table_has_column(conn, "packages", "release_published_at")?;
                let additions = match (has_tag, has_published) {
                    (false, false) => {
                        "
                        ALTER TABLE packages ADD COLUMN release_tag TEXT;
                        ALTER TABLE packages ADD COLUMN release_published_at TEXT;
                    "
                    }
                    (false, true) => "ALTER TABLE packages ADD COLUMN release_tag TEXT;",
                    (true, false) => "ALTER TABLE packages ADD COLUMN release_published_at TEXT;",
                    (true, true) => "",
                };
                conn.execute_batch(&format!(
                    "
                    BEGIN;
                    {additions}
                    UPDATE packages
                    SET release_tag = replace(
                        version_tag_template,
                        '{{}}',
                        version_major || '.' || version_minor || '.' || version_patch
                    )
                    WHERE release_tag IS NULL
                      AND version_tag_template IS NOT NULL
                      AND instr(version_tag_template, '{{}}') > 0
                      AND NOT (version_major = 0 AND version_minor = 0 AND version_patch = 0);
                    PRAGMA user_version = 8;
                    COMMIT;
                    "
                ))
                .context("Failed to migrate package database schema from version 7 to 8")?;
                current_version = 8;
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

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("Failed to inspect table '{table}'"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .with_context(|| format!("Failed to inspect columns for table '{table}'"))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn schema_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .context("Failed to read package database schema version")
}
