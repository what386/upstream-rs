use chrono::{DateTime, Utc};
use std::cmp::Ordering;

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
    pub fn cmp_version_then_published(&self, other: &Self) -> Ordering {
        self.version
            .partial_cmp(&other.version)
            .unwrap_or_else(|| self.published_at.cmp(&other.published_at))
    }

    pub fn is_newer_than(&self, version: &Version, published_at: DateTime<Utc>) -> bool {
        match self.version.partial_cmp(version) {
            Some(Ordering::Greater) => true,
            Some(_) => false,
            None => self.published_at > published_at,
        }
    }

    pub fn get_asset_by_name_invariant(&self, name: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|a| a.name.to_lowercase() == name.to_lowercase())
    }
}
