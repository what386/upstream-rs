pub mod determine;
pub mod downloader;
pub mod profiles;
pub mod scripts;
pub mod worker;

use std::path::PathBuf;

use crate::models::{
    common::{enums::Provider, version::Version},
    provider::Release,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    Rust,
    Dotnet,
    Go,
    Zig,
    Cmake,
}

#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub name: String,
    pub repo_slug: String,
    pub provider: Provider,
    pub base_url: Option<String>,
    pub version_tag: Option<String>,
    pub branch: Option<String>,
    pub requested_profile: Option<BuildProfile>,
    pub build_output: Option<PathBuf>,
    pub script_action: scripts::BuildScriptAction,
}

#[derive(Debug, Clone)]
pub struct BuildOutput {
    pub artifact_path: PathBuf,
    pub profile: BuildProfile,
    pub release: Release,
    pub version: Version,
    pub branch: Option<String>,
    pub commit: Option<String>,
}
