use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::gitea::gitea_client::{GiteaAssetDto, GiteaClient, GiteaReleaseDto};

#[derive(Debug, Clone)]
pub struct GiteaAdapter {
    client: GiteaClient,
}

impl GiteaAdapter {
    pub fn new(client: GiteaClient) -> Self {
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

    pub async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        let dto = self.client.get_release_by_tag(slug, tag).await?;
        Ok(self.convert_release(dto))
    }

    pub async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        let dto = self.client.get_latest_release(slug).await?;
        Ok(self.convert_release(dto))
    }

    pub async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        let dtos = self.client.get_releases(slug, per_page, max_total).await?;
        Ok(dtos
            .into_iter()
            .map(|dto| self.convert_release(dto))
            .collect())
    }

    fn convert_asset(dto: GiteaAssetDto) -> Asset {
        let created_at = Self::parse_timestamp(&dto.created_at);
        Asset::new(
            dto.browser_download_url,
            dto.id as u64,
            dto.name,
            dto.size as u64,
            created_at,
        )
    }

    fn convert_release(&self, dto: GiteaReleaseDto) -> Release {
        let assets: Vec<Asset> = dto.assets.into_iter().map(Self::convert_asset).collect();
        let version =
            Version::from_tag(&dto.tag_name).unwrap_or_else(|_| Version::new(0, 0, 0, false));
        Release {
            id: dto.id as u64,
            tag: dto.tag_name,
            name: dto.name,
            body: dto.body,
            is_draft: dto.draft,
            is_prerelease: dto.prerelease,
            published_at: Self::parse_timestamp(&dto.published_at),
            assets,
            version,
        }
    }

    fn parse_timestamp(raw: &str) -> DateTime<Utc> {
        if raw.trim().is_empty() {
            return DateTime::<Utc>::MIN_UTC;
        }
        raw.parse::<DateTime<Utc>>()
            .unwrap_or(DateTime::<Utc>::MIN_UTC)
    }
}
