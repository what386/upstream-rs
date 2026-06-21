#![allow(async_fn_in_trait)]

use std::path::Path;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::models::{
    common::Version,
    provider::{Asset, Release, RepositorySearchFilters, RepositorySearchResult},
};

#[async_trait(?Send)]
pub trait ReleaseProvider {
    async fn get_latest_release(&self, slug: &str) -> Result<Release>;

    async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>>;

    async fn get_releases_newer_than(
        &self,
        slug: &str,
        from_version: &Version,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        let releases = self.get_releases(slug, per_page, None).await?;
        Ok(releases
            .into_iter()
            .filter(|release| release.version > *from_version)
            .collect())
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release>;

    async fn get_branch_head_sha(&self, _slug: &str, _branch: &str) -> Result<String> {
        Err(anyhow!("Branch builds are not supported for this provider"))
    }

    async fn get_project_readme(&self, _slug: &str) -> Result<String> {
        Err(anyhow!("Project README is not supported for this provider"))
    }

    async fn search_repositories(
        &self,
        _query: &str,
        _limit: Option<u32>,
        _filters: &RepositorySearchFilters,
    ) -> Result<Vec<RepositorySearchResult>> {
        Err(anyhow!(
            "Repository search is not supported for this provider"
        ))
    }

    async fn get_latest_release_if_modified_since(
        &self,
        slug: &str,
        _last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        Ok(Some(self.get_latest_release(slug).await?))
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()>;
}

#[async_trait(?Send)]
impl<T> ReleaseProvider for &T
where
    T: ReleaseProvider + ?Sized,
{
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        (*self).get_latest_release(slug).await
    }

    async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        (*self).get_releases(slug, per_page, max_total).await
    }

    async fn get_releases_newer_than(
        &self,
        slug: &str,
        from_version: &Version,
        per_page: Option<u32>,
    ) -> Result<Vec<Release>> {
        (*self)
            .get_releases_newer_than(slug, from_version, per_page)
            .await
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        (*self).get_release_by_tag(slug, tag).await
    }

    async fn get_branch_head_sha(&self, slug: &str, branch: &str) -> Result<String> {
        (*self).get_branch_head_sha(slug, branch).await
    }

    async fn get_project_readme(&self, slug: &str) -> Result<String> {
        (*self).get_project_readme(slug).await
    }

    async fn search_repositories(
        &self,
        query: &str,
        limit: Option<u32>,
        filters: &RepositorySearchFilters,
    ) -> Result<Vec<RepositorySearchResult>> {
        (*self).search_repositories(query, limit, filters).await
    }

    async fn get_latest_release_if_modified_since(
        &self,
        slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        (*self)
            .get_latest_release_if_modified_since(slug, last_upgraded)
            .await
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        (*self)
            .download_asset(asset, destination_path, dl_callback)
            .await
    }
}
