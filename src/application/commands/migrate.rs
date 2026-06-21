use anyhow::Result;

use crate::{output, routines::migrate, utils::static_paths::UpstreamPaths};

pub fn run() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let report = migrate::run(&paths)?;

    println!("{}", output::title("Migration"));
    output::status_line(
        output::Status::Ok,
        "directories",
        format!("created {}", report.created_dirs),
    );
    output::status_line(
        output::Status::Ok,
        "packages",
        format!("moved {}", report.moved_entries),
    );
    output::status_line(
        output::Status::Ok,
        "metadata",
        format!("updated {}", report.updated_packages),
    );
    output::status_line(
        output::Status::Ok,
        "rollback",
        format!("updated {}", report.updated_rollback_records),
    );
    output::status_line(
        output::Status::Ok,
        "trust",
        format!(
            "imported {}, deduped {}",
            report.migrated_trusted_keys, report.deduped_trusted_keys
        ),
    );
    output::status_line(
        output::Status::Ok,
        "symlinks",
        format!(
            "refreshed {}, skipped {}",
            report.refreshed_symlinks, report.skipped_symlinks
        ),
    );
    println!("{}", output::success("Migration complete."));

    Ok(())
}
