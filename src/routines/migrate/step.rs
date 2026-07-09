use anyhow::Result;

use crate::{routines::migrate::MigrationReport, utils::static_paths::UpstreamPaths};

pub trait Step {
    fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
        if Self::check(paths)? {
            Self::apply(paths, report)?;
        }

        Ok(())
    }

    fn check(paths: &UpstreamPaths) -> Result<bool>;

    fn apply(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()>;
}
