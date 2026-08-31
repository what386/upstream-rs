use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, Transaction, params};

use crate::models::upstream::{Package, PackageExecutable};

pub(super) fn load_executables(conn: &Connection, package: &mut Package) -> Result<()> {
    package.executables = list_executables(conn, &package.id)?;
    Ok(())
}

pub(super) fn load_executables_for_packages(
    conn: &Connection,
    packages: &mut [Package],
) -> Result<()> {
    for package in packages {
        load_executables(conn, package)?;
    }
    Ok(())
}

fn list_executables(conn: &Connection, package_name: &str) -> Result<Vec<PackageExecutable>> {
    let mut statement = conn
        .prepare(
            "SELECT path, name FROM package_executables
             WHERE package_id = ?1 ORDER BY name",
        )
        .with_context(|| format!("Failed to prepare executable query for '{package_name}'"))?;

    statement
        .query_map([package_name], |row| {
            Ok(PackageExecutable {
                path: PathBuf::from(row.get::<_, String>(0)?),
                name: row.get(1)?,
            })
        })
        .with_context(|| format!("Failed to load executables for '{package_name}'"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("Failed to decode executable aliases")
}

pub(super) fn replace_executables(tx: &Transaction<'_>, package: &Package) -> Result<()> {
    tx.execute(
        "DELETE FROM package_executables WHERE package_id = ?1",
        [&package.id],
    )
    .with_context(|| format!("Failed to clear executable aliases for '{}'", package.id))?;

    for executable in &package.executables {
        let path = executable.path.to_str().ok_or_else(|| {
            anyhow!(
                "Executable path '{}' is not valid UTF-8",
                executable.path.display()
            )
        })?;
        tx.execute(
            "INSERT INTO package_executables (package_id, path, name) VALUES (?1, ?2, ?3)",
            params![package.id, path, executable.name],
        )
        .with_context(|| {
            format!(
                "Failed to store executable alias '{}' for package '{}'",
                executable.name, package.id
            )
        })?;
    }

    Ok(())
}
