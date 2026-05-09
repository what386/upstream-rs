use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::models::common::enums::{Channel, Provider};
use crate::models::provider::{Asset, Release, RepositorySearchResult};
use crate::models::upstream::Package;
use crate::providers::asset_selector::{AssetCandidate, AssetSelector};
use crate::providers::gitea::{GiteaAdapter, GiteaClient};
use crate::providers::github::{GithubAdapter, GithubClient};
use crate::providers::gitlab::{GitlabAdapter, GitlabClient};
use crate::providers::http::{DirectAdapter, HttpClient, WebScraperAdapter};
use crate::providers::release_provider::ReleaseProvider;

use anyhow::{Result, anyhow};

pub struct ProviderManager {
    github_token: Option<String>,
    gitlab_token: Option<String>,
    gitea_token: Option<String>,

    github: OnceLock<GithubAdapter>,
    gitlab: OnceLock<GitlabAdapter>,
    gitea: OnceLock<GiteaAdapter>,
    http: OnceLock<WebScraperAdapter>,
    direct: OnceLock<DirectAdapter>,

    asset_selector: AssetSelector,
}

impl ProviderManager {
    pub fn new(
        github_token: Option<&str>,
        gitlab_token: Option<&str>,
        gitea_token: Option<&str>,
    ) -> Result<Self> {
        Ok(Self {
            github_token: github_token.map(str::to_string),
            gitlab_token: gitlab_token.map(str::to_string),
            gitea_token: gitea_token.map(str::to_string),
            github: OnceLock::new(),
            gitlab: OnceLock::new(),
            gitea: OnceLock::new(),
            http: OnceLock::new(),
            direct: OnceLock::new(),
            asset_selector: AssetSelector::new(),
        })
    }

    // Construction is idempotent; a redundant build on concurrent init is acceptable.
    // Applies to all adapter builds.
    fn github_adapter(&self) -> Result<&GithubAdapter> {
        if let Some(adapter) = self.github.get() {
            return Ok(adapter);
        }

        let adapter = GithubClient::new(self.github_token.as_deref()).map(GithubAdapter::new)?;
        Ok(self.github.get_or_init(|| adapter))
    }

    fn gitlab_adapter(&self) -> Result<&GitlabAdapter> {
        if let Some(adapter) = self.gitlab.get() {
            return Ok(adapter);
        }

        let adapter =
            GitlabClient::new(self.gitlab_token.as_deref(), None).map(GitlabAdapter::new)?;
        Ok(self.gitlab.get_or_init(|| adapter))
    }

    fn gitea_adapter(&self) -> Result<&GiteaAdapter> {
        if let Some(adapter) = self.gitea.get() {
            return Ok(adapter);
        }

        let adapter = GiteaClient::new(self.gitea_token.as_deref(), None).map(GiteaAdapter::new)?;
        Ok(self.gitea.get_or_init(|| adapter))
    }

    fn webscraper_adapter(&self) -> Result<&WebScraperAdapter> {
        if let Some(adapter) = self.http.get() {
            return Ok(adapter);
        }

        let adapter = HttpClient::new().map(WebScraperAdapter::new)?;
        Ok(self.http.get_or_init(|| adapter))
    }

    fn direct_adapter(&self) -> Result<&DirectAdapter> {
        if let Some(adapter) = self.direct.get() {
            return Ok(adapter);
        }

        let adapter = HttpClient::new().map(DirectAdapter::new)?;
        Ok(self.direct.get_or_init(|| adapter))
    }

    fn resolve_provider(
        &self,
        provider: &Provider,
        base_url: Option<&str>,
    ) -> Result<Box<dyn ReleaseProvider + '_>> {
        match provider {
            Provider::Github => Ok(Box::new(self.github_adapter()?)),
            Provider::Gitlab => {
                if let Some(base) = base_url {
                    let adapter = GitlabAdapter::new(GitlabClient::new(
                        self.gitlab_token.as_deref(),
                        Some(base),
                    )?);
                    Ok(Box::new(adapter))
                } else {
                    Ok(Box::new(self.gitlab_adapter()?))
                }
            }
            Provider::Gitea => {
                if let Some(base) = base_url {
                    let adapter = GiteaAdapter::new(GiteaClient::new(
                        self.gitea_token.as_deref(),
                        Some(base),
                    )?);
                    Ok(Box::new(adapter))
                } else {
                    Ok(Box::new(self.gitea_adapter()?))
                }
            }
            Provider::WebScraper => Ok(Box::new(self.webscraper_adapter()?)),
            Provider::Direct => Ok(Box::new(self.direct_adapter()?)),
        }
    }

    pub async fn get_latest_release(
        &self,
        slug: &str,
        provider: &Provider,
        channel: &Channel,
        base_url: Option<&str>,
    ) -> Result<Release> {
        match channel {
            Channel::Stable => {
                self.get_latest_stable_release(slug, provider, base_url)
                    .await
            }
            Channel::Preview => {
                self.get_latest_preview_release(slug, provider, base_url)
                    .await
            }
            Channel::Nightly => {
                self.get_latest_nightly_release(slug, provider, base_url)
                    .await
            }
        }
    }

    pub async fn check_for_updates(&self, package: &Package) -> Result<Option<Release>> {
        let resolved = self.resolve_provider(&package.provider, package.base_url.as_deref())?;
        resolved
            .get_latest_release_if_modified_since(&package.repo_slug, Some(package.last_upgraded))
            .await
    }

    pub fn is_nightly_release(tag: &str) -> bool {
        tag.to_lowercase().contains("nightly")
    }

    pub fn is_preview_release(release: &Release) -> bool {
        release.is_prerelease && !Self::is_nightly_release(&release.tag)
    }

    pub async fn get_latest_nightly_release(
        &self,
        slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
    ) -> Result<Release> {
        let releases = self
            .get_releases(slug, provider, Some(20), Some(20), base_url)
            .await?;

        releases
            .into_iter()
            .filter(|r| !r.is_draft)
            .filter(|r| Self::is_nightly_release(&r.tag))
            .max_by(|a, b| a.version.cmp(&b.version))
            .ok_or_else(|| anyhow!("No nightly releases found for '{}'.", slug))
    }

    pub async fn get_latest_preview_release(
        &self,
        slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
    ) -> Result<Release> {
        let releases = self
            .get_releases(slug, provider, Some(20), Some(20), base_url)
            .await?;

        releases
            .into_iter()
            .filter(|r| !r.is_draft)
            .filter(Self::is_preview_release)
            .max_by(|a, b| a.version.cmp(&b.version))
            .ok_or_else(|| anyhow!("No preview releases found for '{}'.", slug))
    }

    pub async fn get_latest_stable_release(
        &self,
        slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
    ) -> Result<Release> {
        let resolved = self.resolve_provider(provider, base_url)?;
        resolved.get_latest_release(slug).await
    }

    pub async fn get_releases(
        &self,
        slug: &str,
        provider: &Provider,
        per_page: Option<u32>,
        max_total: Option<u32>,
        base_url: Option<&str>,
    ) -> Result<Vec<Release>> {
        let resolved = self.resolve_provider(provider, base_url)?;
        resolved.get_releases(slug, per_page, max_total).await
    }

    pub async fn get_release_by_tag(
        &self,
        slug: &str,
        tag: &str,
        provider: &Provider,
        base_url: Option<&str>,
    ) -> Result<Release> {
        let resolved = self.resolve_provider(provider, base_url)?;
        resolved.get_release_by_tag(slug, tag).await
    }

    pub async fn search_repositories(
        &self,
        query: &str,
        provider: &Provider,
        limit: Option<u32>,
        base_url: Option<&str>,
    ) -> Result<Vec<RepositorySearchResult>> {
        let resolved = self.resolve_provider(provider, base_url)?;
        resolved.search_repositories(query, limit).await
    }

    pub async fn get_branch_head_sha(
        &self,
        slug: &str,
        provider: &Provider,
        branch: &str,
        base_url: Option<&str>,
    ) -> Result<String> {
        // Keep this explicit guard for a clear forge-only error before provider resolution.
        if matches!(provider, Provider::WebScraper | Provider::Direct) {
            return Err(anyhow!(
                "Branch builds support forge providers only (github/gitlab/gitea)"
            ));
        }

        let resolved = self.resolve_provider(provider, base_url)?;
        resolved.get_branch_head_sha(slug, branch).await
    }

    pub async fn download_asset<F>(
        &self,
        asset: &Asset,
        provider: &Provider,
        cache_path: &Path,
        dl_progress: &mut Option<F>,
    ) -> Result<PathBuf>
    where
        F: FnMut(u64, u64),
    {
        let file_name = Path::new(&asset.name)
            .file_name()
            .ok_or_else(|| anyhow!("Invalid asset name: {}", asset.name))?;

        fs::create_dir_all(cache_path)?;

        let download_filepath = cache_path.join(file_name);
        let resolved = self.resolve_provider(provider, None)?;
        let callback = dl_progress
            .as_mut()
            .map(|cb| cb as &mut dyn FnMut(u64, u64));
        resolved
            .download_asset(asset, &download_filepath, callback)
            .await?;

        Ok(download_filepath)
    }

    pub fn find_recommended_asset(&self, release: &Release, package: &Package) -> Result<Asset> {
        self.asset_selector.find_recommended_asset(release, package)
    }

    pub fn get_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Vec<AssetCandidate>> {
        self.asset_selector.get_candidate_assets(release, package)
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderManager;
    use crate::models::common::{Version, enums::Provider};
    use crate::models::provider::Release;
    use chrono::Utc;

    fn make_release(prerelease: bool, tag: &str) -> Release {
        Release {
            id: 1,
            tag: tag.to_string(),
            name: tag.to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: prerelease,
            assets: Vec::new(),
            version: Version::new(1, 0, 0, prerelease),
            published_at: Utc::now(),
        }
    }

    #[test]
    fn nightly_release_detection_is_case_insensitive() {
        assert!(ProviderManager::is_nightly_release("Nightly-20260221"));
        assert!(!ProviderManager::is_nightly_release("v1.2.3"));
    }

    #[test]
    fn preview_release_excludes_nightly_tags() {
        let preview = make_release(true, "v1.2.3-rc1");
        let nightly = make_release(true, "nightly-20260221");

        assert!(ProviderManager::is_preview_release(&preview));
        assert!(!ProviderManager::is_preview_release(&nightly));
    }

    #[tokio::test]
    async fn search_repositories_is_unsupported_for_non_github_providers() {
        let manager = ProviderManager::new(None, None, None).expect("provider manager");
        let err = manager
            .search_repositories("ripgrep", &Provider::Gitlab, Some(5), None)
            .await
            .expect_err("gitlab search should be unsupported");

        assert!(
            err.to_string()
                .contains("Repository search is not supported"),
            "unexpected error: {err}"
        );
    }
}
