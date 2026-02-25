use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};

use super::gitlab_client::GitlabClient;
use super::gitlab_dtos::GitlabReleaseDto;

#[derive(Debug, Clone)]
pub struct GitlabAdapter {
    client: GitlabClient,
}

impl GitlabAdapter {
    pub fn new(client: GitlabClient) -> Self {
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

    pub async fn get_release_by_tag(&self, project_path: &str, tag: &str) -> Result<Release> {
        let dto = self.client.get_release_by_tag(project_path, tag).await?;
        Ok(self.convert_release(dto))
    }

    pub async fn get_latest_release(&self, project_path: &str) -> Result<Release> {
        let releases = self.get_releases(project_path, Some(1), Some(1)).await?;
        releases
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No releases found for project {}", project_path))
    }

    pub async fn get_releases(
        &self,
        project_path: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        let dtos = self
            .client
            .get_releases(project_path, per_page, max_total)
            .await?;
        Ok(dtos
            .into_iter()
            .map(|dto| self.convert_release(dto))
            .collect())
    }

    fn convert_release(&self, dto: GitlabReleaseDto) -> Release {
        let mut assets = Vec::new();
        let mut asset_id: u64 = 0;

        // Convert asset links to Assets
        for link in dto.assets.links {
            asset_id += 1;
            let download_url = link.direct_asset_url.unwrap_or(link.url);
            let created_at = Self::parse_timestamp(&dto.created_at);

            assets.push(Asset::new(
                download_url,
                asset_id,
                link.name,
                0, // GitLab doesn't provide size in link metadata
                created_at,
            ));
        }

        // Convert source archives to Assets
        for source in dto.assets.sources {
            asset_id += 1;
            let name = format!("source.{}", source.format);
            let created_at = Self::parse_timestamp(&dto.created_at);

            assets.push(Asset::new(source.url, asset_id, name, 0, created_at));
        }

        let version =
            Version::from_tag(&dto.tag_name).unwrap_or_else(|_| Version::new(0, 0, 0, false));
        let published_at = dto
            .released_at
            .as_ref()
            .map(|s| Self::parse_timestamp(s))
            .unwrap_or_else(|| Self::parse_timestamp(&dto.created_at));

        Release {
            id: asset_id, // GitLab doesn't have numeric release IDs
            tag: dto.tag_name,
            name: dto.name,
            body: dto.description,
            is_draft: false, // GitLab doesn't have draft releases
            is_prerelease: dto.upcoming_release.unwrap_or(false),
            published_at,
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

#[cfg(test)]
#[path = "../../../tests/providers/gitlab/gitlab_adapter.rs"]
mod tests;
