use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::release_provider::ReleaseProvider;

use super::gitea_client::GiteaClient;
use super::gitea_dtos::{GiteaAssetDto, GiteaReleaseDto};

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

    pub async fn get_branch_head_sha(&self, slug: &str, branch: &str) -> Result<String> {
        self.client.get_branch_head_sha(slug, branch).await
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

impl ReleaseProvider for GiteaAdapter {
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        GiteaAdapter::get_latest_release(self, slug).await
    }

    async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        GiteaAdapter::get_releases(self, slug, per_page, max_total).await
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        GiteaAdapter::get_release_by_tag(self, slug, tag).await
    }

    async fn get_branch_head_sha(&self, slug: &str, branch: &str) -> Result<String> {
        GiteaAdapter::get_branch_head_sha(self, slug, branch).await
    }

    async fn download_asset<F>(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: &mut Option<F>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
    {
        GiteaAdapter::download_asset(self, asset, destination_path, dl_callback).await
    }
}

#[cfg(test)]
mod tests {
    use super::GiteaAdapter;
    use crate::providers::gitea::gitea_client::GiteaClient;
    use crate::providers::gitea::gitea_dtos::{GiteaAssetDto, GiteaReleaseDto};

    #[test]
    fn parse_timestamp_returns_min_on_invalid_inputs() {
        assert_eq!(
            GiteaAdapter::parse_timestamp(""),
            chrono::DateTime::<chrono::Utc>::MIN_UTC
        );
        assert_eq!(
            GiteaAdapter::parse_timestamp("bad"),
            chrono::DateTime::<chrono::Utc>::MIN_UTC
        );
    }

    #[test]
    fn convert_release_maps_core_fields() {
        let adapter = GiteaAdapter::new(GiteaClient::new(None, None).expect("gitea client"));
        let dto = GiteaReleaseDto {
            id: 7,
            tag_name: "v3.1.4".to_string(),
            name: "release".to_string(),
            body: "notes".to_string(),
            prerelease: false,
            draft: true,
            published_at: "2026-02-21T00:00:00Z".to_string(),
            assets: vec![GiteaAssetDto {
                id: 10,
                name: "tool-linux.tar.gz".to_string(),
                browser_download_url: "https://example.invalid/tool-linux.tar.gz".to_string(),
                size: 55,
                content_type: "application/gzip".to_string(),
                created_at: "2026-02-20T00:00:00Z".to_string(),
            }],
        };

        let release = adapter.convert_release(dto);
        assert_eq!(release.id, 7);
        assert_eq!(release.version.to_string(), "3.1.4");
        assert!(release.is_draft);
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].id, 10);
    }
}
