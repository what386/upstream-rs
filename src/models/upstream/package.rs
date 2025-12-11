
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::models::common::version::Version;
use crate::models::common::enums::{Channel, Filetype, Provider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub repo_slug: String,

    pub pkg_kind: Filetype,
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
    pub fn new(
        name: String,
        repo_slug: String,

        pkg_kind: Filetype,
        version: Version,
        channel: Channel,
        provider: Provider,

        has_icon: bool,
        is_paused: bool,
        install_path: Option<PathBuf>,
        exec_path: Option<PathBuf>,

        last_upgraded: DateTime<Utc>,
    ) -> Self {
        Self {
            name,
            repo_slug,

            pkg_kind,
            version,
            channel,
            provider,

            has_icon,
            is_paused,
            install_path,
            exec_path,

            last_upgraded,
        }
    }

    pub fn with_defaults(
        name: String,
        repo_slug: String,
        pkg_kind: Filetype,
        version: Version,
        channel: Channel,
        provider: Provider,
    ) -> Self {
        let now = Utc::now();
        Self {
            name,
            repo_slug,

            pkg_kind,
            version,
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

    /// Get a human-readable display name for the package.
    pub fn display_name(&self) -> String {
        format!("{} ({}:{})", self.name, self.channel, self.repo_slug)
    }
}
