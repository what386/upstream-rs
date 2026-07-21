use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::Row;
use serde::{Serialize, de::DeserializeOwned};

use crate::models::common::Version;
use crate::models::upstream::Package;
use crate::providers::pattern_matcher::PatternTable;

pub const PACKAGE_COLUMNS: &str = "
    name,
    repo_slug,
    filetype,
    version_major,
    version_minor,
    version_patch,
    version_is_prerelease,
    version_kind,
    version_value,
    release_tag,
    release_published_at,
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
    last_upgraded";

pub(super) fn row_to_package(row: &Row<'_>) -> rusqlite::Result<Package> {
    let version_major: u32 = row.get(3)?;
    let version_minor: u32 = row.get(4)?;
    let version_patch: u32 = row.get(5)?;
    let version_is_prerelease: bool = db_bool(row.get(6)?);
    let version_kind: String = row.get(7)?;
    let version_value: Option<String> = row.get(8)?;
    let release_published_at: Option<String> = row.get(10)?;
    let last_upgraded: String = row.get(22)?;

    let version = match version_kind.as_str() {
        "Unknown" => Version::new(0, 0, 0, false),
        "Semver" => Version::new(
            version_major,
            version_minor,
            version_patch,
            version_is_prerelease,
        ),
        "Datetime" => {
            let value = version_value.ok_or_else(|| {
                rusqlite::Error::InvalidColumnType(
                    8,
                    "version_value".to_string(),
                    rusqlite::types::Type::Null,
                )
            })?;
            Version::parse(&value).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        err.to_string(),
                    )),
                )
            })?
        }
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                7,
                "version_kind".to_string(),
                rusqlite::types::Type::Text,
            ));
        }
    };

    Ok(Package {
        name: row.get(0)?,
        repo_slug: row.get(1)?,
        filetype: enum_from_db_value(row.get::<_, String>(2)?, 2)?,
        version,
        release_tag: row.get(9)?,
        release_published_at: release_published_at
            .map(|timestamp| parse_timestamp(timestamp, 10))
            .transpose()?,
        version_tag_template: row.get(11)?,
        channel: enum_from_db_value(row.get::<_, String>(12)?, 12)?,
        provider: enum_from_db_value(row.get::<_, String>(13)?, 13)?,
        base_url: row.get(14)?,
        install_type: enum_from_db_value(row.get::<_, String>(15)?, 15)?,
        build_branch: row.get(16)?,
        build_commit: row.get(17)?,
        is_pinned: db_bool(row.get(18)?),
        match_pattern: PatternTable::empty(),
        exclude_pattern: PatternTable::empty(),
        icon_path: optional_path_from_db(row.get(19)?),
        install_path: optional_path_from_db(row.get(20)?),
        exec_path: optional_path_from_db(row.get(21)?),
        last_upgraded: parse_timestamp(last_upgraded, 22)?,
    })
}

pub(super) fn enum_to_db<T>(value: &T) -> Result<String>
where
    T: Serialize,
{
    let serialized =
        serde_json::to_value(value).context("Failed to serialize enum for database")?;
    serialized
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Enum did not serialize to a string"))
}

pub(super) fn optional_path_to_db(path: &Option<PathBuf>) -> Result<Option<String>> {
    path.as_ref()
        .map(|path| {
            path.to_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| anyhow!("Path '{}' is not valid UTF-8", path.display()))
        })
        .transpose()
}

pub(super) fn bool_to_db(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

pub(super) fn enum_from_db_value<T>(value: String, column: usize) -> rusqlite::Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(err),
        )
    })
}

fn optional_path_from_db(path: Option<String>) -> Option<PathBuf> {
    path.map(PathBuf::from)
}

fn db_bool(value: i64) -> bool {
    value != 0
}

fn parse_timestamp(value: String, column: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(err),
            )
        })
}
