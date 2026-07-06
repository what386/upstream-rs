mod v2_0_0;
mod v2_3_0;
mod v2_6_0;
mod v2_11_0;

use anyhow::Result;

use crate::{routines::migrate::MigrationReport, utils::static_paths::UpstreamPaths};

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    v2_0_0::run(paths, report)?;
    v2_3_0::run(paths, report)?;
    v2_6_0::run(paths, report)?;
    v2_11_0::run(paths, report)?;
    Ok(())
}
