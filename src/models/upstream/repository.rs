use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::models::common::version::String;
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
        provider: Provider,
        channel: Channel,
        last_updated: DateTime<Utc>,
        latest_tag: Option<String>,
    ) -> Self {
        let slug = format!("{}{}", name, owner);
        Self {
            name,
            owner,
            slug: slug,
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
        channel: Option<Channel>,
    ) -> Self {
        let now = Utc::now();
        let slug = format!("{}{}", name, owner);
        Self {
            name,
            owner,
            slug: slug,
            provider,
            channel: channel.unwrap_or(Channel::Stable),
            last_updated: now,
            latest_tag: None,
        }
    }

    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}
