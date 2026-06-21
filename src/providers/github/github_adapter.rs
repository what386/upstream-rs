use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release, RepositorySearchFilters, RepositorySearchResult};
use crate::providers::release_provider::ReleaseProvider;

use super::github_client::GithubClient;
use super::github_dtos::{GithubAssetDto, GithubReleaseDto, GithubRepositorySearchItemDto};

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

    pub async fn get_releases_newer_than(
        &self,
        slug: &str,
        from_version: &Version,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        let per_page = per_page.unwrap_or(30);
        let mut page = 1;
        let mut releases = Vec::new();

        loop {
            let batch = self.client.get_releases_page(slug, per_page, page).await?;
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

    pub async fn get_branch_head_sha(&self, slug: &str, branch: &str) -> Result<String> {
        self.client.get_branch_head_sha(slug, branch).await
    }

    pub async fn get_project_readme(&self, slug: &str) -> Result<String> {
        self.client.get_project_readme(slug).await
    }

    pub async fn search_repositories(
        &self,
        query: &str,
        limit: Option<u32>,
        filters: &RepositorySearchFilters,
    ) -> Result<Vec<RepositorySearchResult>> {
        let dto = self
            .client
            .search_repositories(query, limit, filters)
            .await?;
        Ok(dto
            .items
            .into_iter()
            .map(Self::convert_search_result)
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

    fn convert_search_result(dto: GithubRepositorySearchItemDto) -> RepositorySearchResult {
        RepositorySearchResult {
            repo_slug: dto.full_name,
            display_name: dto.name,
            description: dto.description,
            stars: dto.stargazers_count,
            language: dto.language,
            updated_at: Self::parse_timestamp(&dto.updated_at),
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
impl ReleaseProvider for GithubAdapter {
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        GithubAdapter::get_latest_release(self, slug).await
    }

    async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        GithubAdapter::get_releases(self, slug, per_page, max_total).await
    }

    async fn get_releases_newer_than(
        &self,
        slug: &str,
        from_version: &Version,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        GithubAdapter::get_releases_newer_than(self, slug, from_version, per_page).await
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        GithubAdapter::get_release_by_tag(self, slug, tag).await
    }

    async fn get_branch_head_sha(&self, slug: &str, branch: &str) -> Result<String> {
        GithubAdapter::get_branch_head_sha(self, slug, branch).await
    }

    async fn get_project_readme(&self, slug: &str) -> Result<String> {
        GithubAdapter::get_project_readme(self, slug).await
    }

    async fn search_repositories(
        &self,
        query: &str,
        limit: Option<u32>,
        filters: &RepositorySearchFilters,
    ) -> Result<Vec<RepositorySearchResult>> {
        GithubAdapter::search_repositories(self, query, limit, filters).await
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        let mut forwarded = dl_callback;
        GithubAdapter::download_asset(self, asset, destination_path, &mut forwarded).await
    }
}

#[cfg(test)]
mod tests {
    use super::GithubAdapter;
    use crate::providers::github::github_client::GithubClient;
    use crate::providers::github::github_dtos::{
        GithubAssetDto, GithubReleaseDto, GithubRepositorySearchItemDto,
    };

    #[test]
    fn parse_timestamp_returns_min_for_invalid_or_empty_values() {
        assert_eq!(
            GithubAdapter::parse_timestamp(""),
            chrono::DateTime::<chrono::Utc>::MIN_UTC
        );
        assert_eq!(
            GithubAdapter::parse_timestamp("not-a-date"),
            chrono::DateTime::<chrono::Utc>::MIN_UTC
        );
    }

    #[test]
    fn convert_release_maps_assets_and_version() {
        let adapter =
            GithubAdapter::new(GithubClient::new(None, Default::default()).expect("github client"));
        let dto = GithubReleaseDto {
            id: 12,
            tag_name: "v2.3.4".to_string(),
            name: "Release 2.3.4".to_string(),
            body: "notes".to_string(),
            prerelease: true,
            draft: false,
            published_at: "2026-02-21T00:00:00Z".to_string(),
            assets: vec![GithubAssetDto {
                id: 9,
                name: "tool-linux-x86_64.tar.gz".to_string(),
                browser_download_url: "https://example.invalid/tool-linux-x86_64.tar.gz"
                    .to_string(),
                size: 123,
                content_type: "application/gzip".to_string(),
                created_at: "2026-02-20T00:00:00Z".to_string(),
            }],
        };

        let release = adapter.convert_release(dto);
        assert_eq!(release.id, 12);
        assert_eq!(release.version.to_string(), "2.3.4");
        assert!(release.is_prerelease);
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].id, 9);
    }

    #[test]
    fn convert_search_result_maps_fields() {
        let dto = GithubRepositorySearchItemDto {
            full_name: "BurntSushi/ripgrep".to_string(),
            name: "ripgrep".to_string(),
            description: "fast grep".to_string(),
            stargazers_count: 123,
            language: "Rust".to_string(),
            updated_at: "2026-05-09T00:00:00Z".to_string(),
            archived: false,
            fork: false,
        };

        let result = GithubAdapter::convert_search_result(dto);
        assert_eq!(result.repo_slug, "BurntSushi/ripgrep");
        assert_eq!(result.display_name, "ripgrep");
        assert_eq!(result.description, "fast grep");
        assert_eq!(result.stars, 123);
        assert_eq!(result.language, "Rust");
    }

    #[test]
    fn convert_search_result_invalid_timestamp_uses_min() {
        let dto = GithubRepositorySearchItemDto {
            full_name: "owner/repo".to_string(),
            name: "repo".to_string(),
            description: String::new(),
            stargazers_count: 0,
            language: String::new(),
            updated_at: "nope".to_string(),
            archived: false,
            fork: false,
        };

        let result = GithubAdapter::convert_search_result(dto);
        assert_eq!(result.updated_at, chrono::DateTime::<chrono::Utc>::MIN_UTC);
    }
}
