use crate::models::{common::Version, provider::Release};
use anyhow::Result;
use jiff::Timestamp;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub(super) struct IndexVersion {
    pub name: String,
    #[serde(rename = "vers")]
    pub num: String,
    #[serde(rename = "cksum")]
    pub checksum: String,
    pub yanked: bool,
    pub rust_version: Option<String>,
    pub pubtime: Option<Timestamp>,
}

impl IndexVersion {
    pub fn release(&self) -> Result<Release> {
        let version = Version::parse_strict_semver(&self.num)?;
        Ok(Release {
            id: 0,
            tag: self.num.clone(),
            name: self.num.clone(),
            body: String::new(),
            is_draft: false,
            is_prerelease: version.is_prerelease(),
            assets: Vec::new(),
            version,
            published_at: self.pubtime.unwrap_or(Timestamp::MIN),
        })
    }
}

#[derive(Deserialize)]
pub(super) struct SearchResponse {
    pub crates: Vec<SearchCrate>,
    pub meta: SearchMeta,
}

#[derive(Deserialize)]
pub(super) struct SearchMeta {
    pub total: usize,
}

#[derive(Deserialize)]
pub(super) struct SearchCrate {
    pub name: String,
    pub description: Option<String>,
    pub downloads: u64,
    pub max_version: String,
    pub updated_at: Timestamp,
}
