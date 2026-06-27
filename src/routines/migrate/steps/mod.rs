#[path = "v2.0.0.rs"]
mod v2_0_0;
#[path = "v2.3.0.rs"]
mod v2_3_0;
#[path = "v2.6.0.rs"]
mod v2_6_0;

use anyhow::Result;

use crate::{routines::migrate::MigrationReport, utils::static_paths::UpstreamPaths};

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    v2_0_0::run(paths, report)?;
    v2_3_0::run(paths, report)?;
    v2_6_0::run(paths, report)?;
    Ok(())
}
