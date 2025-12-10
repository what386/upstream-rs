use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::models::common::enums::{Channel, Provider};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub owner: String,
    pub slug: String,

    pub provider: Provider,
    pub channel: Channel,

    pub last_updated: DateTime<Utc>,
    pub latest_tag: Option<String>,
}

impl Repository {
    pub fn new(
        name: String,
        owner: String,
        slug: String,
        provider: Provider,
        channel: Channel,
        last_updated: DateTime<Utc>,
        latest_tag: Option<String>,
    ) -> Self {
        Self {
            name,
            owner,
            slug,
            provider,
            channel,
            last_updated,
            latest_tag,
        }
    }

    pub fn with_defaults(
        name: String,
        owner: String,
        provider: Provider,
        channel: Channel,
    ) -> Self {
        let now = Utc::now();
        let slug = format!("{}{}", name, owner);
        Self {
            name,
            owner,
            slug: slug,

            provider,
            channel: channel,
            last_updated: now,
            latest_tag: None,
        }
    }

    /// Get a human-readable display name for the repo.
    pub fn display_name(&self) -> String {
        format!("{}: {} ({})", self.provider, self.slug, self.channel)
    }
}
