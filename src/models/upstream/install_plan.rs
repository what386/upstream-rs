use crate::models::common::enums::{BuildProfile, Channel, Filetype, Provider, TrustMode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub name: String,
    pub desktop: bool,
    pub source: InstallSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    Release(ReleaseInstallSource),
    Build(BuildInstallSource),
    Http(HttpInstallSource),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInstallSource {
    pub source: String,
    pub kind: Filetype,
    pub provider: Option<Provider>,
    pub base_url: Option<String>,
    pub channel: Channel,
    pub selector: ReleaseSelector,
    pub match_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
    pub trust_mode: TrustMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpInstallSource {
    pub url: String,
    pub kind: Filetype,
    pub trust_mode: TrustMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInstallSource {
    pub source: String,
    pub provider: Option<Provider>,
    pub base_url: Option<String>,
    pub channel: Channel,
    pub selector: BuildSelector,
    pub profile: Option<BuildProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseSelector {
    Latest,
    Tag(String),
    Semver(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildSelector {
    Latest,
    Tag(String),
    Semver(String),
    Branch(String),
}

impl ReleaseSelector {
    pub fn from_options(tag: Option<String>, semver: Option<String>) -> Self {
        match (tag, semver) {
            (Some(tag), None) => Self::Tag(tag),
            (None, Some(semver)) => Self::Semver(semver),
            _ => Self::Latest,
        }
    }

    pub fn into_options(self) -> (Option<String>, Option<String>) {
        match self {
            Self::Latest => (None, None),
            Self::Tag(tag) => (Some(tag), None),
            Self::Semver(semver) => (None, Some(semver)),
        }
    }
}

impl BuildSelector {
    pub fn from_options(
        tag: Option<String>,
        semver: Option<String>,
        branch: Option<String>,
    ) -> Self {
        match (tag, semver, branch) {
            (Some(tag), None, None) => Self::Tag(tag),
            (None, Some(semver), None) => Self::Semver(semver),
            (None, None, Some(branch)) => Self::Branch(branch),
            _ => Self::Latest,
        }
    }

    pub fn into_options(self) -> (Option<String>, Option<String>, Option<String>) {
        match self {
            Self::Latest => (None, None, None),
            Self::Tag(tag) => (Some(tag), None, None),
            Self::Semver(semver) => (None, Some(semver), None),
            Self::Branch(branch) => (None, None, Some(branch)),
        }
    }
}
