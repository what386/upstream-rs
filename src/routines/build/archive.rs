use anyhow::{Context, Result};
use jiff::Timestamp;

use crate::models::{
    common::enums::{Channel, Provider},
    provider::{Asset, Release},
};
use crate::services::artifact::compression_handler;

use super::downloader::{SourceDownload, SourceDownloader};

impl<'a> SourceDownloader<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn fetch_source_from_archive(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        channel: &Channel,
        tag: Option<&str>,
        branch: Option<&str>,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<SourceDownload> {
        if let Some(branch_name) = branch {
            Self::emit_status(
                status_callback,
                format!("Fetching branch head for '{branch_name}' ..."),
            );
            let head_commit = self
                .provider_manager
                .get_branch_head_sha(repo_slug, provider, branch_name, base_url)
                .await
                .context(format!(
                    "Failed to fetch branch head for '{}' on '{}'",
                    branch_name, repo_slug
                ))?;

            let release = branch_release(branch_name);
            let workspace_path = self
                .download_and_extract(
                    &self
                        .make_source_archive_asset(repo_slug, provider, branch_name, base_url)
                        .await?,
                    provider,
                    status_callback,
                )
                .await?;

            Self::emit_status(status_callback, "Resolving source workspace ...");
            let workspace_path = Self::resolve_workspace_root(&workspace_path)?;
            let workspace_path = self.cache_archive_workspace(
                repo_slug,
                provider,
                base_url,
                branch_name,
                &workspace_path,
                status_callback,
            )?;

            return Ok(SourceDownload {
                workspace_path,
                release,
                branch: Some(branch_name.to_string()),
                commit: Some(head_commit),
            });
        }

        let release = if let Some(tag_name) = tag {
            Self::emit_status(
                status_callback,
                format!("Fetching release metadata for '{tag_name}' ..."),
            );
            self.provider_manager
                .get_release_by_tag(repo_slug, tag_name, provider, base_url)
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'",
                    tag_name, repo_slug
                ))?
        } else {
            Self::emit_status(status_callback, "Fetching latest release metadata ...");
            self.provider_manager
                .get_latest_release(repo_slug, provider, channel, base_url)
                .await
                .context(format!("fetch latest release for '{}'", repo_slug))?
        };

        let primary = self
            .make_source_archive_asset(repo_slug, provider, &release.tag, base_url)
            .await?;

        let downloaded_primary = self
            .download_asset(&primary, provider, status_callback)
            .await;

        let downloaded = match downloaded_primary {
            Ok(path) => path,
            Err(primary_err) => {
                if let Some(fallback) = find_release_source_asset(&release) {
                    Self::emit_status(status_callback, "Trying release source asset fallback ...");
                    self.download_asset(fallback, provider, status_callback)
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

        let extracted_path = self.extract(downloaded, status_callback)?;
        Self::emit_status(status_callback, "Resolving source workspace ...");
        let workspace_path = self.cache_archive_workspace(
            repo_slug,
            provider,
            base_url,
            &release.tag,
            &Self::resolve_workspace_root(&extracted_path)?,
            status_callback,
        )?;

        Ok(SourceDownload {
            workspace_path,
            release,
            branch: None,
            commit: None,
        })
    }

    async fn download_and_extract(
        &self,
        asset: &Asset,
        provider: &Provider,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<std::path::PathBuf> {
        let downloaded = self
            .download_asset(asset, provider, status_callback)
            .await?;

        self.extract(downloaded, status_callback)
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        provider: &Provider,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<std::path::PathBuf> {
        Self::emit_status(status_callback, "Downloading source archive ...");
        let mut progress = Some(|downloaded, total| {
            Self::emit_download_status(status_callback, downloaded, total)
        });

        self.provider_manager
            .download_asset(asset, provider, &self.archive_cache_dir, &mut progress)
            .await
    }

    fn extract(
        &self,
        downloaded: std::path::PathBuf,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<std::path::PathBuf> {
        let extract_root = self.archive_cache_dir.join("extract");
        std::fs::create_dir_all(&extract_root).context(format!(
            "Failed to create extraction root '{}'",
            extract_root.display()
        ))?;
        Self::emit_status(status_callback, "Unpacking source archive ...");
        compression_handler::decompress(&downloaded, &extract_root)
            .context("Failed to unpack source archive")
    }

    async fn make_source_archive_asset(
        &self,
        repo_slug: &str,
        provider: &Provider,
        git_ref: &str,
        base_url: Option<&str>,
    ) -> Result<Asset> {
        self.provider_manager
            .get_source_archive(repo_slug, provider, git_ref, base_url)
            .await
    }
}

fn branch_release(branch: &str) -> Release {
    Release {
        id: 0,
        tag: branch.to_string(),
        name: format!("branch {branch}"),
        body: String::new(),
        is_draft: false,
        is_prerelease: false,
        published_at: Timestamp::now(),
        assets: vec![],
        version: crate::models::common::version::Version::new(0, 0, 0, false),
    }
}

fn find_release_source_asset(release: &Release) -> Option<&Asset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.starts_with("source."))
}
