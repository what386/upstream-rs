use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::release_provider::ReleaseProvider;

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

    pub async fn get_releases_newer_than(
        &self,
        project_path: &str,
        from_version: &Version,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        let per_page = per_page.unwrap_or(20).min(100);
        let mut page = 1;
        let mut releases = Vec::new();

        loop {
            let batch = self
                .client
                .get_releases_page(project_path, per_page, page)
                .await?;
            if batch.is_empty() {
                break;
            }

            let partial_page = batch.len() < per_page as usize;
            let mut reached_from_version = false;
            for dto in batch {
                let parsed_version = Version::from_tag(&dto.tag_name).ok();
                let release = self.convert_release(dto);
                if parsed_version
                    .as_ref()
                    .is_some_and(|version| version <= from_version)
                {
                    reached_from_version = true;
                    continue;
                }
                releases.push(release);
            }

            if reached_from_version || partial_page {
                break;
            }

            page += 1;
        }

        Ok(releases)
    }

    pub async fn get_branch_head_sha(&self, project_path: &str, branch: &str) -> Result<String> {
        self.client.get_branch_head_sha(project_path, branch).await
    }

    pub async fn get_project_readme(&self, project_path: &str) -> Result<String> {
        self.client.get_project_readme(project_path).await
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

#[async_trait::async_trait(?Send)]
impl ReleaseProvider for GitlabAdapter {
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        GitlabAdapter::get_latest_release(self, slug).await
    }

    async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        GitlabAdapter::get_releases(self, slug, per_page, max_total).await
    }

    async fn get_releases_newer_than(
        &self,
        slug: &str,
        from_version: &Version,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        GitlabAdapter::get_releases_newer_than(self, slug, from_version, per_page).await
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        GitlabAdapter::get_release_by_tag(self, slug, tag).await
    }

    async fn get_branch_head_sha(&self, slug: &str, branch: &str) -> Result<String> {
        GitlabAdapter::get_branch_head_sha(self, slug, branch).await
    }

    async fn get_project_readme(&self, slug: &str) -> Result<String> {
        GitlabAdapter::get_project_readme(self, slug).await
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        let mut forwarded = dl_callback;
        GitlabAdapter::download_asset(self, asset, destination_path, &mut forwarded).await
    }
}

#[cfg(test)]
mod tests {
    use super::GitlabAdapter;
    use crate::providers::gitlab::gitlab_client::GitlabClient;
    use crate::providers::gitlab::gitlab_dtos::{
        GitlabAssetsDto, GitlabLinkDto, GitlabReleaseDto, GitlabSourceDto,
    };

    #[test]
    fn parse_timestamp_handles_invalid_values() {
        assert_eq!(
            GitlabAdapter::parse_timestamp(""),
            chrono::DateTime::<chrono::Utc>::MIN_UTC
        );
        assert_eq!(
            GitlabAdapter::parse_timestamp("bad-date"),
            chrono::DateTime::<chrono::Utc>::MIN_UTC
        );
    }

    #[test]
    fn convert_release_combines_links_and_sources_into_assets() {
        let adapter = GitlabAdapter::new(
            GitlabClient::new(None, None, Default::default()).expect("gitlab client"),
        );
        let dto = GitlabReleaseDto {
            tag_name: "v1.9.0".to_string(),
            name: "v1.9.0".to_string(),
            description: "notes".to_string(),
            created_at: "2026-02-21T00:00:00Z".to_string(),
            released_at: None,
            upcoming_release: Some(false),
            assets: GitlabAssetsDto {
                count: 2,
                links: vec![GitlabLinkDto {
                    id: 1,
                    name: "tool-linux.tar.gz".to_string(),
                    url: "https://example.invalid/tool-linux.tar.gz".to_string(),
                    direct_asset_url: None,
                    link_type: None,
                }],
                sources: vec![GitlabSourceDto {
                    format: "tar.gz".to_string(),
                    url: "https://example.invalid/source.tar.gz".to_string(),
                }],
            },
        };

        let release = adapter.convert_release(dto);
        assert_eq!(release.version.to_string(), "1.9.0");
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.assets[1].name, "source.tar.gz");
    }
}
