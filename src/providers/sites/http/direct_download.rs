use anyhow::{Result, anyhow, bail};
use jiff::Timestamp;
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::sites::ReleaseProvider;
use crate::providers::sites::http::http_client::{ConditionalProbeResult, HttpClient};

#[derive(Debug, Clone)]
pub struct DirectAdapter {
    client: HttpClient,
}

impl DirectAdapter {
    fn parse_version_from_filename(filename: &str) -> Option<Version> {
        Version::from_filename(filename).ok()
    }

    fn version_from_last_modified(dt: Timestamp) -> Version {
        let dt = dt.to_zoned_datetime(jiff::tz::TimeZone::UTC);
        let major = dt.year() as u32;
        let minor = dt.day_of_year() as u32;
        let patch = dt.hour() as u32 * 3600 + dt.minute() as u32 * 60 + dt.second() as u32;
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
        self.get_latest_release_since(slug, None)
            .await?
            .ok_or_else(|| anyhow!("Unexpected not-modified response for direct provider"))
    }

    pub async fn get_latest_release_since(
        &self,
        slug: &str,
        last_upgraded: Option<Timestamp>,
    ) -> Result<Option<Release>> {
        let probe = self
            .client
            .probe_asset_if_modified_since(slug, last_upgraded)
            .await?;

        let info = match probe {
            ConditionalProbeResult::NotModified => return Ok(None),
            ConditionalProbeResult::Asset(info) => info,
        };

        let published_at = info
            .last_modified
            .unwrap_or_else(|| last_upgraded.unwrap_or_else(Timestamp::now));

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

#[async_trait::async_trait(?Send)]
impl ReleaseProvider for DirectAdapter {
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        DirectAdapter::get_latest_release(self, slug).await
    }

    async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        DirectAdapter::get_releases(self, slug, per_page, max_total).await
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        DirectAdapter::get_release_by_tag(self, slug, tag).await
    }

    async fn get_latest_release_since(
        &self,
        slug: &str,
        last_upgraded: Option<Timestamp>,
    ) -> Result<Option<Release>> {
        DirectAdapter::get_latest_release_since(self, slug, last_upgraded).await
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        let mut forwarded = dl_callback;
        DirectAdapter::download_asset(self, asset, destination_path, &mut forwarded).await
    }
}
