use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};

use crate::services::providers::github::github_client::{
    GithubAssetDto, GithubClient, GithubReleaseDto,
};

#[derive(Debug, Clone)]
pub struct GithubAdapter {
    client: GithubClient,
}

impl GithubAdapter {
    pub fn new(client: GithubClient) -> Self {
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

    pub async fn get_release_by_id(&self, slug: &str, release_id: i64) -> Result<Release> {
        let dto = self.client.get_release_by_id(slug, release_id).await?;
        Ok(self.convert_release(dto))
    }

    pub async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        let dto = self.client.get_latest_release(slug).await?;
        Ok(self.convert_release(dto))
    }

    pub async fn get_all_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        let dtos = self.client.get_all_releases(slug, per_page).await?;
        Ok(dtos
            .into_iter()
            .map(|dto| self.convert_release(dto))
            .collect())
    }

    fn convert_asset(dto: GithubAssetDto) -> Asset {
        let created_at = Self::parse_timestamp(&dto.created_at);
        Asset::new(
            dto.browser_download_url,
            dto.id as u64,
            dto.name,
            dto.size as u64,
            created_at,
        )
    }

    fn convert_release(&self, dto: GithubReleaseDto) -> Release {
        let assets: Vec<Asset> = dto.assets.into_iter().map(Self::convert_asset).collect();
        let version = Self::parse_version(&dto.tag_name);
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

    fn parse_version(tag: &str) -> Version {
        let tag = tag.trim();
        let tag = tag
            .strip_prefix('v')
            .or_else(|| tag.strip_prefix('V'))
            .unwrap_or(tag);

        const PREFIXES: &[&str] = &["release-", "rel-", "ver-", "version-"];
        let cleaned = PREFIXES
            .iter()
            .find_map(|prefix| {
                tag.to_lowercase()
                    .strip_prefix(prefix)
                    .map(|_| &tag[prefix.len()..])
            })
            .unwrap_or(tag);

        Version::parse(cleaned).unwrap_or_else(|_| Version::new(0, 0, 0, false))
    }

    fn parse_timestamp(raw: &str) -> DateTime<Utc> {
        if raw.trim().is_empty() {
            return DateTime::<Utc>::MIN_UTC;
        }
        raw.parse::<DateTime<Utc>>()
            .unwrap_or(DateTime::<Utc>::MIN_UTC)
    }
}
