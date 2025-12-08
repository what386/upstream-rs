use std::fmt::format;

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::models::common::version::Version;
use crate::models::common::enums::{Channel, Provider, Filetype};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub repo_slug: String,
    pub id: String,

    pub pkg_kind: Filetype,
    pub version: Version,
    pub channel: Channel,
    pub provider: Provider,

    pub has_icon: bool,
    pub is_paused: bool,
    pub install_path: Option<String>,
    pub exec_path: Option<String>,

    pub last_upgraded: DateTime<Utc>,
}

impl Package {
    pub fn new(
        name: String,
        repo_slug: String,
        id: String,

        pkg_kind: Filetype,
        version: Version,
        channel: Channel,
        provider: Provider,

        has_icon: bool,
        is_paused: bool,
        install_path: Option<String>,
        exec_path: Option<String>,

        last_upgraded: DateTime<Utc>,
    ) -> Self {
        Self {
            name,
            repo_slug,
            id,

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
        provider: Provider,
        channel: Channel,
    ) -> Self {
        let now = Utc::now();
        let id = Self::generate_id(&provider, &repo_slug, &channel, &name);
        Self {
            name,
            repo_slug,
            id,

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

    /// Generate a deterministic ID based on provider, repo_slug, and channel.
    /// This ensures the same package from the same source always has the same ID.
    /// Format: "provider:repo_slug:channel:name"
    fn generate_id(provider: &Provider, repo_slug: &str, channel: &Channel, name: &str) -> String {
        format!("{}:{}:{}:{}",
            provider.to_string(),
            repo_slug.to_lowercase(),
            channel.to_string(),
            name.to_string(),
        )
    }

    /// Check if this package is the same as another (same provider, repo, and channel).
    pub fn is_same(&self, other: &Self) -> bool {
        self.id == other.id
    }

    /// Get a human-readable display name for the package.
    pub fn display_name(&self) -> String {
        format!("{} ({})", self.name, self.channel)
    }

    /// Update the package ID if any identifying fields change.
    /// Call this after modifying provider, repo_slug, or channel.
    pub fn refresh_id(&mut self) {
        self.id = Self::generate_id(&self.provider, &self.repo_slug, &self.channel, &self.name);
    }}
