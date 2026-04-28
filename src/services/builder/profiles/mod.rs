use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::services::builder::BuildProfile;

pub mod dotnet;
pub mod rust;

pub trait BuildProfileHandler {
    fn profile(&self) -> BuildProfile;
    fn detect(&self, workspace: &Path) -> bool;
    fn run_build(
        &self,
        workspace: &Path,
        package_name: &str,
        output_override: Option<&Path>,
    ) -> Result<PathBuf>;
}
