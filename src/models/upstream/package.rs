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

    pub is_pinned: bool,
    pub pattern: Option<String>,
    pub icon_path: Option<PathBuf>,
    pub install_path: Option<PathBuf>,
    pub exec_path: Option<PathBuf>,

    pub last_upgraded: DateTime<Utc>,
}

impl Package {
    pub fn with_defaults(
        name: String,
        repo_slug: String,
        pkg_kind: Filetype,
        pattern: Option<String>,
        channel: Channel,
        provider: Provider,
    ) -> Self {
        let now = Utc::now();
        let version = Version::new(0, 0, 0, false);
        Self {
            name,
            repo_slug,

            filetype: pkg_kind,
            version,
            channel,
            provider,

            is_pinned: false,
            pattern,
            icon_path: None,
            install_path: None,
            exec_path: None,

            last_upgraded: now,
        }
    }

    pub fn is_same_as(&self, other: &Package) -> bool {
        self.provider == other.provider
            && self.repo_slug == other.repo_slug
            && self.channel == other.channel
            && self.name == other.name
    }
}
