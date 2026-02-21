use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::http::http_client::{ConditionalProbeResult, HttpClient};

#[derive(Debug, Clone)]
pub struct DirectAdapter {
    client: HttpClient,
}

impl DirectAdapter {
    fn parse_version_from_filename(filename: &str) -> Option<Version> {
        Version::from_filename(filename).ok()
    }

    fn version_from_last_modified(dt: DateTime<Utc>) -> Version {
        let major = dt.year_ce().1;
        let minor = dt.ordinal();
        let patch = dt.num_seconds_from_midnight();
        Version::new(major, minor, patch, false)
    }

    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }

    pub async fn download_asset<F>(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: &mut Option<F>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
    {
        self.client
            .download_file(&asset.download_url, destination_path, dl_callback)
            .await
    }

    pub async fn get_release_by_tag(&self, _slug: &str, _tag: &str) -> Result<Release> {
        bail!("Direct provider does not support tagged releases")
    }

    pub async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        self.get_latest_release_if_modified_since(slug, None)
            .await?
            .ok_or_else(|| anyhow!("Unexpected not-modified response for direct provider"))
    }

    pub async fn get_latest_release_if_modified_since(
        &self,
        slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        let probe = self
            .client
            .probe_asset_if_modified_since(slug, last_upgraded)
            .await?;
        let info = match probe {
            ConditionalProbeResult::NotModified => return Ok(None),
            ConditionalProbeResult::Asset(info) => info,
        };
        let published_at = info.last_modified.unwrap_or_else(Utc::now);
        let version = Self::parse_version_from_filename(&info.name)
            .or_else(|| info.last_modified.map(Self::version_from_last_modified))
            .unwrap_or_else(|| Version::new(0, 0, 0, false));

        let asset = Asset::new(
            info.download_url,
            1,
            info.name.clone(),
            info.size,
            published_at,
        );

        let release_name = if let Some(etag) = info.etag {
            format!("{} [{}]", info.name, etag)
        } else {
            info.name
        };

        Ok(Some(Release {
            id: 1,
            tag: "direct".to_string(),
            name: release_name,
            body: "Direct HTTP asset".to_string(),
            is_draft: false,
            is_prerelease: false,
            assets: vec![asset],
            version,
            published_at,
        }))
    }

    pub async fn get_releases(
        &self,
        slug: &str,
        _per_page: Option<u32>,
        _max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        Ok(vec![self.get_latest_release(slug).await?])
    }
}

#[cfg(test)]
#[path = "../../../tests/providers/http/direct_adapter.rs"]
mod tests;
