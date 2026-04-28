use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;

use crate::models::{
    common::enums::{Channel, Provider},
    provider::{Asset, Release},
};
use crate::providers::provider_manager::ProviderManager;
use crate::services::integration::compression_handler;

pub struct SourceDownload {
    pub workspace_path: PathBuf,
    pub release: Release,
}

pub struct SourceDownloader<'a> {
    provider_manager: &'a ProviderManager,
    cache_dir: PathBuf,
}

impl<'a> SourceDownloader<'a> {
    pub fn new(provider_manager: &'a ProviderManager) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let cache_dir = std::env::temp_dir().join(format!("upstream-build-{nonce}"));
        std::fs::create_dir_all(&cache_dir)
            .context(format!("Failed to create build cache '{}'", cache_dir.display()))?;

        Ok(Self {
            provider_manager,
            cache_dir,
        })
    }

    pub async fn fetch_source(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        channel: &Channel,
        tag: Option<&str>,
    ) -> Result<SourceDownload> {
        let release = if let Some(tag_name) = tag {
            self.provider_manager
                .get_release_by_tag_for(repo_slug, tag_name, provider, base_url)
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'",
                    tag_name, repo_slug
                ))?
        } else {
            self.provider_manager
                .get_latest_release_for(repo_slug, provider, channel, base_url)
                .await
                .context(format!("Failed to fetch latest release for '{}'", repo_slug))?
        };

        let primary_archive =
            self.make_source_archive_asset(repo_slug, provider, &release, base_url)?;
        let mut no_progress: Option<fn(u64, u64)> = None;
        let downloaded_primary = self
            .provider_manager
            .download_asset(&primary_archive, provider, &self.cache_dir, &mut no_progress)
            .await
            .context(format!("Failed to download source archive for '{}'", repo_slug));

        let downloaded = match downloaded_primary {
            Ok(path) => path,
            Err(primary_err) => {
                if let Some(fallback) = Self::find_release_source_asset(&release) {
                    self.provider_manager
                        .download_asset(fallback, provider, &self.cache_dir, &mut no_progress)
                        .await
                        .context(format!(
                            "Failed source download for '{}' using provider endpoint and release source asset fallback: {}",
                            repo_slug, primary_err
                        ))?
                } else {
                    return Err(primary_err);
                }
            }
        };

        let extract_root = self.cache_dir.join("extract");
        std::fs::create_dir_all(&extract_root).context(format!(
            "Failed to create extraction root '{}'",
            extract_root.display()
        ))?;

        let workspace_path = compression_handler::decompress(&downloaded, &extract_root)
            .context("Failed to unpack source archive")?;

        Ok(SourceDownload {
            workspace_path,
            release,
        })
    }

    fn make_source_archive_asset(
        &self,
        repo_slug: &str,
        provider: &Provider,
        release: &Release,
        base_url: Option<&str>,
    ) -> Result<Asset> {
        let url = match provider {
            Provider::Github => format!(
                "https://api.github.com/repos/{}/tarball/{}",
                repo_slug, release.tag
            ),
            Provider::Gitlab => {
                let base = base_url.unwrap_or("https://gitlab.com");
                let encoded = repo_slug.replace('/', "%2F");
                format!(
                    "{}/api/v4/projects/{}/repository/archive.tar.gz?sha={}",
                    base, encoded, release.tag
                )
            }
            Provider::Gitea => {
                let base = base_url.unwrap_or("https://gitea.com");
                format!(
                    "{}/api/v1/repos/{}/archive/{}.tar.gz",
                    base, repo_slug, release.tag
                )
            }
            Provider::Direct | Provider::WebScraper => {
                return Err(anyhow!(
                    "Build supports forge providers only (github/gitlab/gitea)"
                ));
            }
        };

        let asset_name = format!("{}-{}.tar.gz", repo_slug.replace('/', "-"), release.tag);
        Ok(Asset::new(url, 0, asset_name, 0, Utc::now()))
    }

    fn find_release_source_asset(release: &Release) -> Option<&Asset> {
        release
            .assets
            .iter()
            .find(|asset| asset.name.starts_with("source."))
    }
}

impl<'a> Drop for SourceDownloader<'a> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.cache_dir);
    }
}
