use std::fs;
use std::path::{Path, PathBuf};

use crate::models::common::enums::{Channel, Provider};
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::providers::asset_selector::{AssetCandidate, AssetSelector};
use crate::providers::gitea::{GiteaAdapter, GiteaClient};
use crate::providers::github::{GithubAdapter, GithubClient};
use crate::providers::gitlab::{GitlabAdapter, GitlabClient};
use crate::providers::http::{DirectAdapter, HttpClient, WebScraperAdapter};

use anyhow::{Result, anyhow};

pub struct ProviderManager {
    github: GithubAdapter,
    gitlab: GitlabAdapter,
    gitea: GiteaAdapter,
    http: WebScraperAdapter,
    direct: DirectAdapter,
    asset_selector: AssetSelector,
}

impl ProviderManager {
    pub fn new(
        github_token: Option<&str>,
        gitlab_token: Option<&str>,
        gitea_token: Option<&str>,
        provider_base_url: Option<&str>,
    ) -> Result<Self> {
        let github_client = GithubClient::new(github_token)?;
        let gitlab_client = GitlabClient::new(gitlab_token, provider_base_url)?;
        let gitea_client = GiteaClient::new(gitea_token, provider_base_url)?;
        let http_client = HttpClient::new()?;

        let github = GithubAdapter::new(github_client);
        let gitlab = GitlabAdapter::new(gitlab_client);
        let gitea = GiteaAdapter::new(gitea_client);
        let http = WebScraperAdapter::new(http_client.clone());
        let direct = DirectAdapter::new(http_client);

        Ok(Self {
            github,
            gitlab,
            gitea,
            http,
            direct,
            asset_selector: AssetSelector::new(),
        })
    }

    pub async fn get_latest_release(
        &self,
        slug: &str,
        provider: &Provider,
        channel: &Channel,
    ) -> Result<Release> {
        match channel {
            Channel::Stable => self.get_latest_stable_release(slug, provider).await,
            Channel::Preview => self.get_latest_preview_release(slug, provider).await,
            Channel::Nightly => self.get_latest_nightly_release(slug, provider).await,
        }
    }

    pub async fn check_for_updates(&self, package: &Package) -> Result<Option<Release>> {
        match &package.provider {
            Provider::WebScraper => {
                self.http
                    .get_latest_release_if_modified_since(
                        &package.repo_slug,
                        Some(package.last_upgraded),
                    )
                    .await
            }
            Provider::Direct => {
                self.direct
                    .get_latest_release_if_modified_since(
                        &package.repo_slug,
                        Some(package.last_upgraded),
                    )
                    .await
            }
            _ => Ok(Some(
                self.get_latest_release(&package.repo_slug, &package.provider, &package.channel)
                    .await?,
            )),
        }
    }

    /// Detect nightly releases by tag text.
    ///
    /// This intentionally uses a substring check because providers tag nightlies
    /// with inconsistent prefixes/suffixes.
    pub fn is_nightly_release(tag: &str) -> bool {
        tag.to_lowercase().contains("nightly")
    }

    /// Detect preview releases while excluding nightly tags.
    pub fn is_preview_release(release: &Release) -> bool {
        release.is_prerelease && !Self::is_nightly_release(&release.tag)
    }

    pub async fn get_latest_nightly_release(
        &self,
        slug: &str,
        provider: &Provider,
    ) -> Result<Release> {
        let releases = self
            .get_releases(slug, provider, Some(20), Some(20))
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
    ) -> Result<Release> {
        let releases = self
            .get_releases(slug, provider, Some(20), Some(20))
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
    ) -> Result<Release> {
        match provider {
            Provider::Github => self.github.get_latest_release(slug).await,
            Provider::Gitlab => self.gitlab.get_latest_release(slug).await,
            Provider::Gitea => self.gitea.get_latest_release(slug).await,
            Provider::WebScraper => self.http.get_latest_release(slug).await,
            Provider::Direct => self.direct.get_latest_release(slug).await,
        }
    }

    pub async fn get_releases(
        &self,
        slug: &str,
        provider: &Provider,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        match provider {
            Provider::Github => self.github.get_releases(slug, per_page, max_total).await,
            Provider::Gitlab => self.gitlab.get_releases(slug, per_page, max_total).await,
            Provider::Gitea => self.gitea.get_releases(slug, per_page, max_total).await,
            Provider::WebScraper => self.http.get_releases(slug, per_page, max_total).await,
            Provider::Direct => self.direct.get_releases(slug, per_page, max_total).await,
        }
    }

    pub async fn get_release_by_tag(
        &self,
        slug: &str,
        tag: &str,
        provider: &Provider,
    ) -> Result<Release> {
        match provider {
            Provider::Github => self.github.get_release_by_tag(slug, tag).await,
            Provider::Gitlab => self.gitlab.get_release_by_tag(slug, tag).await,
            Provider::Gitea => self.gitea.get_release_by_tag(slug, tag).await,
            Provider::WebScraper => self.http.get_release_by_tag(slug, tag).await,
            Provider::Direct => self.direct.get_release_by_tag(slug, tag).await,
        }
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

        match provider {
            Provider::Github => {
                self.github
                    .download_asset(asset, &download_filepath, dl_progress)
                    .await?
            }
            Provider::Gitlab => {
                self.gitlab
                    .download_asset(asset, &download_filepath, dl_progress)
                    .await?
            }
            Provider::Gitea => {
                self.gitea
                    .download_asset(asset, &download_filepath, dl_progress)
                    .await?
            }
            Provider::WebScraper => {
                self.http
                    .download_asset(asset, &download_filepath, dl_progress)
                    .await?
            }
            Provider::Direct => {
                self.direct
                    .download_asset(asset, &download_filepath, dl_progress)
                    .await?
            }
        }

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
    use crate::models::common::Version;
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
}
