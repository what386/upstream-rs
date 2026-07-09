use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::{Connection, Transaction, params};

use crate::models::upstream::Package;
use crate::providers::pattern_matcher::PatternTable;

pub(super) fn replace_patterns(tx: &Transaction<'_>, package: &Package) -> Result<()> {
    tx.execute(
        "DELETE FROM patterns WHERE package_name = ?1",
        [&package.name],
    )
    .with_context(|| format!("Failed to replace patterns for package '{}'", package.name))?;
    write_patterns(tx, &package.name, "match", &package.match_pattern)?;
    write_patterns(tx, &package.name, "exclude", &package.exclude_pattern)?;
    Ok(())
}

pub(super) fn load_patterns(conn: &Connection, package: &mut Package) -> Result<()> {
    package.match_pattern = load_pattern_kind(conn, &package.name, "match")?;
    package.exclude_pattern = load_pattern_kind(conn, &package.name, "exclude")?;
    Ok(())
}

pub(super) fn load_patterns_for_packages(
    conn: &Connection,
    packages: &mut [Package],
) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT package_name, kind, pattern
             FROM patterns
             ORDER BY package_name, kind, position ASC",
        )
        .context("Failed to prepare bulk pattern query")?;

    let mut patterns_by_package: HashMap<String, PackagePatterns> = HashMap::new();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("Failed to load package patterns")?;

    for row in rows {
        let (package_name, kind, pattern) = row.context("Failed to decode package pattern row")?;
        let patterns = patterns_by_package.entry(package_name).or_default();
        match kind.as_str() {
            "match" => patterns.match_patterns.push(pattern),
            "exclude" => patterns.exclude_patterns.push(pattern),
            _ => {}
        }
    }

    for package in packages {
        if let Some(patterns) = patterns_by_package.remove(&package.name) {
            package.match_pattern = PatternTable::from_patterns(patterns.match_patterns);
            package.exclude_pattern = PatternTable::from_patterns(patterns.exclude_patterns);
        }
    }

    Ok(())
}

#[derive(Default)]
struct PackagePatterns {
    match_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
}

fn write_patterns(
    tx: &Transaction<'_>,
    package_name: &str,
    kind: &str,
    patterns: &PatternTable,
) -> Result<()> {
    let mut stmt = tx
        .prepare(
            "INSERT INTO patterns (package_name, kind, position, pattern)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .context("Failed to prepare pattern insert")?;

    for (position, pattern) in patterns.as_slice().iter().enumerate() {
        stmt.execute(params![package_name, kind, position as u32, pattern])
            .with_context(|| format!("Failed to write {kind} pattern for '{package_name}'"))?;
    }

    Ok(())
}

fn load_pattern_kind(conn: &Connection, package_name: &str, kind: &str) -> Result<PatternTable> {
    let mut stmt = conn
        .prepare(
            "SELECT pattern
             FROM patterns
             WHERE package_name = ?1 AND kind = ?2
             ORDER BY position ASC",
        )
        .with_context(|| format!("Failed to prepare {kind} pattern query"))?;
    let patterns = stmt
        .query_map(params![package_name, kind], |row| row.get::<_, String>(0))
        .with_context(|| format!("Failed to load {kind} patterns for '{package_name}'"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .with_context(|| format!("Failed to decode {kind} patterns for '{package_name}'"))?;
    Ok(PatternTable::from_patterns(patterns))
}
