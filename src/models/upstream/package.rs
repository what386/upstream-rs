
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::models::common::version::Version;
use crate::models::common::enums::{Channel, Filetype, Provider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub repo_slug: String,

    pub package_kind: Filetype,
    pub version: Version,
    pub channel: Channel,
    pub provider: Provider,

    pub has_icon: bool,
    pub is_paused: bool,
    pub install_path: Option<PathBuf>,
    pub exec_path: Option<PathBuf>,

    pub last_upgraded: DateTime<Utc>,
}

impl Package {
    pub fn new_with_defaults(
        name: String,
        repo_slug: String,
        pkg_kind: Filetype,
        channel: Channel,
        provider: Provider,
    ) -> Self {
        let now = Utc::now();
        let version = Version::new(0, 0, 0, false);
        Self {
            name,
            repo_slug,

            package_kind: pkg_kind,
            version: version,
            channel,
            provider,

            has_icon: false,
            is_paused: false,
            install_path: None,
            exec_path: None,

            last_upgraded: now,
        }
    }

    pub fn is_same_as(&self, other: &Package) -> bool {
        self.provider == other.provider &&
        self.repo_slug == other.repo_slug &&
        self.channel == other.channel &&
        self.name == other.name
    }

    pub fn display_name(&self) -> String {
        format!("{} ({}:{})", self.name, self.channel, self.repo_slug)
    }
}
