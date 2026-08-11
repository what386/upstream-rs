use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use indicatif::HumanBytes;

use crate::models::common::enums::{Channel, Provider};
use crate::models::provider::Release;
use crate::providers::provider_manager::ProviderManager;
use crate::utils::static_paths::UpstreamPaths;

pub struct SourceDownload {
    pub workspace_path: PathBuf,
    pub release: Release,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

pub struct SourceDownloader<'a> {
    pub(super) provider_manager: &'a ProviderManager,
    pub(super) cache_dir: PathBuf,
    pub(super) source_archive_cache_dir: PathBuf,
    pub(super) archive_cache_dir: PathBuf,
}

impl<'a> SourceDownloader<'a> {
    pub fn new(provider_manager: &'a ProviderManager, paths: &UpstreamPaths) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let cache_dir = paths.dirs.cache_dir.join("build");
        let source_archive_cache_dir = paths.dirs.cache_dir.join("source");
        let archive_cache_dir = std::env::temp_dir().join(format!("upstream-build-{nonce}"));
        std::fs::create_dir_all(&cache_dir).context(format!(
            "Failed to create build cache '{}'",
            cache_dir.display()
        ))?;
        std::fs::create_dir_all(&source_archive_cache_dir).context(format!(
            "Failed to create source archive cache '{}'",
            source_archive_cache_dir.display()
        ))?;
        std::fs::create_dir_all(&archive_cache_dir).context(format!(
            "Failed to create temporary build archive cache '{}'",
            archive_cache_dir.display()
        ))?;

        Ok(Self {
            provider_manager,
            cache_dir,
            source_archive_cache_dir,
            archive_cache_dir,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_source(
        &self,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
        channel: &Channel,
        tag: Option<&str>,
        branch: Option<&str>,
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
    ) -> Result<SourceDownload> {
        if branch.is_some() && tag.is_some() {
            return Err(anyhow!(
                "Build options --tag and --branch are mutually exclusive"
            ));
        }

        match self
            .fetch_source_from_git(
                repo_slug,
                provider,
                base_url,
                channel,
                tag,
                branch,
                status_callback,
            )
            .await
        {
            Ok(source) => return Ok(source),
            Err(err) => Self::emit_status(
                status_callback,
                format!("Git source cache unavailable, falling back to source archive: {err}"),
            ),
        }

        self.fetch_source_from_archive(
            repo_slug,
            provider,
            base_url,
            channel,
            tag,
            branch,
            status_callback,
        )
        .await
    }

    pub(super) fn emit_status(
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
        status: impl AsRef<str>,
    ) {
        if let Some(callback) = status_callback.as_deref_mut() {
            callback(status.as_ref());
        }
    }

    pub(super) fn emit_download_status(
        status_callback: &mut Option<&mut dyn FnMut(&str)>,
        downloaded: u64,
        total: u64,
    ) {
        let status = if total == 0 {
            format!("Downloading source archive ... {}", HumanBytes(downloaded))
        } else {
            format!(
                "Downloading source archive ... {} / {}",
                HumanBytes(downloaded),
                HumanBytes(total)
            )
        };
        Self::emit_status(status_callback, status);
    }
}

impl<'a> Drop for SourceDownloader<'a> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.archive_cache_dir);
    }
}

fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

pub(super) fn cache_key(base_url: Option<&str>, provider: &Provider, value: &str) -> String {
    let base = base_url
        .map(normalize_base_url)
        .unwrap_or_else(|| provider.to_string());
    sanitize_path_component(&format!("{base}/{value}"))
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
