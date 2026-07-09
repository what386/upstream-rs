mod report;
mod step;
mod steps;

use anyhow::Result;

pub use report::MigrationReport;

use crate::utils::static_paths::UpstreamPaths;

pub fn run(paths: &UpstreamPaths) -> Result<MigrationReport> {
    let mut report = MigrationReport::default();

    steps::run(paths, &mut report)?;

    Ok(report)
}
