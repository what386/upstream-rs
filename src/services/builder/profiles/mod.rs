use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::services::builder::BuildProfile;

pub mod dotnet;
pub mod cmake;
pub mod go;
pub mod rust;
pub mod zig;

pub fn handlers() -> [Box<dyn BuildProfileHandler>; 5] {
    [
        Box::new(rust::RustProfile),
        Box::new(dotnet::DotnetProfile),
        Box::new(go::GoProfile),
        Box::new(zig::ZigProfile),
        Box::new(cmake::CmakeProfile),
    ]
}

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
