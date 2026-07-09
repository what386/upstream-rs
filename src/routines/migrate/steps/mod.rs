mod v2_0_0;
mod v2_11_0;
mod v2_12_0;
mod v2_3_0;
mod v2_6_0;

use anyhow::Result;

use crate::{routines::migrate::MigrationReport, utils::static_paths::UpstreamPaths};

// Each migration should be keyed as "v0_0_0.rs",
// with the semver triplet of the breaking version
// as the step filename, separated by underscores.
// they would then be run from oldest to newest.

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    v2_0_0::run(paths, report)?;
    v2_3_0::run(paths, report)?;
    v2_6_0::run(paths, report)?;
    v2_11_0::run(paths, report)?;
    v2_12_0::run(paths, report)?;
    Ok(())
}
