use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::common::{
    enums::{Channel, Filetype, Provider},
    version::Version,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub repo_slug: String,

    pub filetype: Filetype,
    pub version: Version,
    pub channel: Channel,
    pub provider: Provider,
    pub base_url: Option<String>,

    pub is_pinned: bool,
    pub match_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
    pub icon_path: Option<PathBuf>,
    pub install_path: Option<PathBuf>,
    pub exec_path: Option<PathBuf>,

    pub last_upgraded: DateTime<Utc>,
}

impl Package {
    pub fn with_defaults(
        name: String,
        repo_slug: String,
        filetype: Filetype,
        match_pattern: Option<String>,
        exclude_pattern: Option<String>,
        channel: Channel,
        provider: Provider,
        base_url: Option<String>,
    ) -> Self {
        Self {
            name,
            repo_slug,

            filetype,
            version: Version::new(0, 0, 0, false),
            channel,
            provider,
            base_url,

            is_pinned: false,
            match_pattern,
            exclude_pattern,
            icon_path: None,
            install_path: None,
            exec_path: None,

            last_upgraded: Utc::now(),
        }
    }

    pub fn is_same_as(&self, other: &Package) -> bool {
        self.provider == other.provider
            && self.repo_slug == other.repo_slug
            && self.channel == other.channel
            && self.name == other.name
            && self.base_url == other.base_url
    }
}

#[cfg(test)]
#[path = "../../../tests/models/upstream/package.rs"]
mod tests;
