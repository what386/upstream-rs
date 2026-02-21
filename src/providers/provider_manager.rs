use std::fs;
use std::path::{Path, PathBuf};

use crate::models::common::enums::{Channel, Filetype, Provider};
use crate::models::provider::{Asset, Release};
use crate::models::upstream::Package;
use crate::providers::gitea::{GiteaAdapter, GiteaClient};
use crate::providers::github::{GithubAdapter, GithubClient};
use crate::providers::gitlab::{GitlabAdapter, GitlabClient};
use crate::providers::http::{DirectAdapter, HttpClient, WebScraperAdapter};
use crate::utils::platform_info::{ArchitectureInfo, CpuArch, format_arch, format_os};

use anyhow::{Result, anyhow};

pub struct ProviderManager {
    github: GithubAdapter,
    gitlab: GitlabAdapter,
    gitea: GiteaAdapter,
    http: WebScraperAdapter,
    direct: DirectAdapter,
    architecture_info: ArchitectureInfo,
}

#[derive(Debug, Clone)]
pub struct AssetCandidate {
    pub asset: Asset,
    pub score: i32,
}

impl ProviderManager {
    pub fn new(
        github_token: Option<&str>,
        gitlab_token: Option<&str>,
        gitea_token: Option<&str>,
        provider_base_url: Option<&str>,
    ) -> Result<Self> {
        let architecture_info = ArchitectureInfo::new();

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
            architecture_info,
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
        let target_filetype = if package.filetype == Filetype::Auto {
            Self::resolve_auto_filetype(release)?
        } else {
            package.filetype
        };

        let compatible_assets: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .filter(|a| a.filetype == target_filetype)
            .collect();

        compatible_assets
            .into_iter()
            .max_by_key(|a| self.score_asset(a, package))
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "No compatible assets found for {} on {}",
                    format_arch(&self.architecture_info.cpu_arch),
                    format_os(&self.architecture_info.os_kind)
                )
            })
    }

    pub fn get_candidate_assets(
        &self,
        release: &Release,
        package: &Package,
    ) -> Result<Vec<AssetCandidate>> {
        let target_filetype = if package.filetype == Filetype::Auto {
            Self::resolve_auto_filetype(release)?
        } else {
            package.filetype
        };

        let mut candidates: Vec<AssetCandidate> = release
            .assets
            .iter()
            .filter(|a| self.is_potentially_compatible(a))
            .filter(|a| a.filetype == target_filetype)
            .map(|asset| AssetCandidate {
                asset: asset.clone(),
                score: self.score_asset(asset, package),
            })
            .collect();

        candidates.sort_by(|a, b| b.score.cmp(&a.score));
        Ok(candidates)
    }

    pub fn resolve_auto_filetype(release: &Release) -> Result<Filetype> {
        #[cfg(unix)]
        let priority = [
            Filetype::AppImage,
            Filetype::Archive,
            Filetype::Compressed,
            Filetype::Binary,
        ];

        #[cfg(windows)]
        let priority = [Filetype::WinExe, Filetype::Archive, Filetype::Compressed];

        priority
            .iter()
            .find(|&&filetype| {
                release
                    .assets
                    .iter()
                    .any(|asset| asset.filetype == filetype)
            })
            .copied()
            .ok_or_else(|| anyhow!("No compatible filetype found in release assets"))
    }

    fn is_potentially_compatible(&self, asset: &Asset) -> bool {
        // OS check
        if let Some(target_os) = &asset.target_os
            && *target_os != self.architecture_info.os_kind
        {
            return false;
        }

        // Architecture check
        if let Some(target_arch) = &asset.target_arch {
            if *target_arch == self.architecture_info.cpu_arch {
                return true;
            }

            // Compatibility fallbacks
            if self.architecture_info.cpu_arch == CpuArch::X86_64 && *target_arch == CpuArch::X86 {
                return true;
            }

            if self.architecture_info.cpu_arch == CpuArch::Aarch64 && *target_arch == CpuArch::Arm {
                return true;
            }

            return *target_arch == self.architecture_info.cpu_arch;
        }

        true
    }

    fn score_asset(&self, asset: &Asset, package: &Package) -> i32 {
        let name = asset.name.to_lowercase();
        let mut score = 0;

        // Architecture match bonus
        if let Some(target_arch) = &asset.target_arch {
            if *target_arch == self.architecture_info.cpu_arch {
                score += 80;
            } else if self.architecture_info.cpu_arch == CpuArch::X86_64
                && *target_arch == CpuArch::X86
            {
                score += 30;
            } else if self.architecture_info.cpu_arch == CpuArch::Aarch64
                && *target_arch == CpuArch::Arm
            {
                score += 30;
            }
        }

        // Archive format preference
        if asset.filetype == Filetype::Archive {
            if name.ends_with(".tar.bz2") || name.ends_with(".tbz") {
                score += 15;
            } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
                score += 10;
            } else if name.ends_with(".zip") {
                score += 5;
            }
        }

        // Compression format preference
        if asset.filetype == Filetype::Compressed {
            if name.ends_with(".bz2") {
                score += 10;
            } else if name.ends_with(".gz") {
                score += 5;
            }
        }

        // Binary format preference
        if asset.filetype == Filetype::Binary && Path::new(&name).extension().is_none() {
            score += 10;
        }

        if name.contains("static") {
            score += 5;
        }

        if name.contains("debug") || name.contains("symbols") {
            score -= 20;
        }

        // Package name match
        if !name.contains(&package.name.to_lowercase()) {
            score -= 40;
        }

        // Very small files, or absurdly large files
        if asset.size < 100_000 || asset.size > 500_000_000 {
            score -= 20;
        }

        // User prefs
        if let Some(pattern) = &package.match_pattern
            && name.contains(pattern)
        {
            score += 100;
        }

        if let Some(antipattern) = &package.exclude_pattern
            && name.contains(antipattern)
        {
            score -= 100;
        }

        score
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderManager;
    use crate::models::common::Version;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::provider::{Asset, Release};
    use crate::models::upstream::Package;
    use chrono::Utc;

    fn make_release(assets: Vec<Asset>, prerelease: bool, tag: &str) -> Release {
        Release {
            id: 1,
            tag: tag.to_string(),
            name: tag.to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: prerelease,
            assets,
            version: Version::new(1, 0, 0, prerelease),
            published_at: Utc::now(),
        }
    }

    fn make_package(filetype: Filetype) -> Package {
        Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            filetype,
            Some("static".to_string()),
            Some("debug".to_string()),
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn nightly_release_detection_is_case_insensitive() {
        assert!(ProviderManager::is_nightly_release("Nightly-20260221"));
        assert!(!ProviderManager::is_nightly_release("v1.2.3"));
    }

    #[test]
    fn preview_release_excludes_nightly_tags() {
        let preview = make_release(Vec::new(), true, "v1.2.3-rc1");
        let nightly = make_release(Vec::new(), true, "nightly-20260221");

        assert!(ProviderManager::is_preview_release(&preview));
        assert!(!ProviderManager::is_preview_release(&nightly));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_auto_filetype_prefers_appimage_then_archives_on_unix() {
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool.tar.gz".to_string(),
                    1,
                    "tool.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.AppImage".to_string(),
                    2,
                    "tool.AppImage".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        assert_eq!(
            ProviderManager::resolve_auto_filetype(&release).expect("resolve"),
            Filetype::AppImage
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolve_auto_filetype_prefers_winexe_on_windows() {
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool.tar.gz".to_string(),
                    1,
                    "tool.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool.exe".to_string(),
                    2,
                    "tool.exe".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        assert_eq!(
            ProviderManager::resolve_auto_filetype(&release).expect("resolve"),
            Filetype::WinExe
        );
    }

    #[test]
    fn get_candidate_assets_sorts_by_score_descending() {
        let manager = ProviderManager::new(None, None, None, None).expect("manager");
        let package = make_package(Filetype::Archive);
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-debug.tar.gz".to_string(),
                    1,
                    "tool-debug.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-static.tar.bz2".to_string(),
                    2,
                    "tool-static.tar.bz2".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let candidates = manager
            .get_candidate_assets(&release, &package)
            .expect("candidates");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].asset.name, "tool-static.tar.bz2");
        assert!(candidates[0].score > candidates[1].score);
    }

    #[test]
    fn find_recommended_asset_returns_highest_scored_compatible_asset() {
        let manager = ProviderManager::new(None, None, None, None).expect("manager");
        let package = make_package(Filetype::Archive);
        let release = make_release(
            vec![
                Asset::new(
                    "https://example.invalid/tool-debug.tar.gz".to_string(),
                    1,
                    "tool-debug.tar.gz".to_string(),
                    200_000,
                    Utc::now(),
                ),
                Asset::new(
                    "https://example.invalid/tool-static.tar.bz2".to_string(),
                    2,
                    "tool-static.tar.bz2".to_string(),
                    200_000,
                    Utc::now(),
                ),
            ],
            false,
            "v1.0.0",
        );

        let best = manager
            .find_recommended_asset(&release, &package)
            .expect("best asset");
        assert_eq!(best.name, "tool-static.tar.bz2");
    }
}
