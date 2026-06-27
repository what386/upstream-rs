use chrono::{DateTime, Utc};

use crate::models::common::version::Version;
use crate::models::provider::asset::Asset;

#[derive(Debug, Clone)]
pub struct Release {
    pub id: u64,
    pub tag: String,
    pub name: String,
    pub body: String,

    pub is_draft: bool,
    pub is_prerelease: bool,

    pub assets: Vec<Asset>,
    pub version: Version,

    pub published_at: DateTime<Utc>,
}

impl Release {
    pub fn get_asset_by_name_invariant(&self, name: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|a| a.name.to_lowercase() == name.to_lowercase())
    }
}
