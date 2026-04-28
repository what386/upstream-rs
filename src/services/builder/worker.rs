use anyhow::{Result, anyhow};

use crate::models::common::{enums::Channel, version::Version};
use crate::providers::provider_manager::ProviderManager;
use crate::services::builder::determine::determine_profile;
use crate::services::builder::downloader::SourceDownloader;
use crate::services::builder::profiles::handlers;
use crate::services::builder::{BuildOutput, BuildRequest};

pub struct BuildWorker<'a> {
    provider_manager: &'a ProviderManager,
}

impl<'a> BuildWorker<'a> {
    pub fn new(provider_manager: &'a ProviderManager) -> Self {
        Self { provider_manager }
    }

    pub async fn build(&self, request: BuildRequest, channel: Channel) -> Result<BuildOutput> {
        let downloader = SourceDownloader::new(self.provider_manager)?;
        let source = downloader
            .fetch_source(
                &request.repo_slug,
                &request.provider,
                request.base_url.as_deref(),
                &channel,
                request.version_tag.as_deref(),
            )
            .await?;

        let handlers = handlers();
        let profile = determine_profile(
            &source.workspace_path,
            request.requested_profile,
            &handlers,
        )?;
        let selected = handlers
            .iter()
            .find(|handler| handler.profile() == profile)
            .ok_or_else(|| anyhow!("Unsupported build profile"))?;

        let artifact = selected.run_build(
            &source.workspace_path,
            &request.name,
            request.build_output.as_deref(),
        )?;

        let version = if source.release.version == Version::new(0, 0, 0, false) {
            Version::from_tag(&source.release.tag).unwrap_or_else(|_| Version::new(0, 0, 0, false))
        } else {
            source.release.version.clone()
        };

        Ok(BuildOutput {
            artifact_path: artifact,
            profile,
            release: source.release,
            version,
        })
    }
}
