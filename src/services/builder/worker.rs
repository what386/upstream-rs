use anyhow::{Result, anyhow};

use crate::models::common::{enums::Channel, version::Version};
use crate::providers::provider_manager::ProviderManager;
use crate::services::builder::downloader::SourceDownloader;
use crate::services::builder::profiles::BuildProfileHandler;
use crate::services::builder::profiles::dotnet::DotnetProfile;
use crate::services::builder::profiles::rust::RustProfile;
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

        let rust = RustProfile;
        let dotnet = DotnetProfile;
        let handlers: [&dyn BuildProfileHandler; 2] = [&rust, &dotnet];

        let selected = if let Some(profile) = request.requested_profile {
            handlers
                .iter()
                .find(|handler| handler.profile() == profile)
                .copied()
                .ok_or_else(|| anyhow!("Unsupported build profile"))?
        } else {
            handlers
                .iter()
                .find(|handler| handler.detect(&source.workspace_path))
                .copied()
                .ok_or_else(|| {
                    anyhow!(
                        "Could not auto-detect a build profile for '{}'. Use --build-profile.",
                        request.repo_slug
                    )
                })?
        };

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
            profile: selected.profile(),
            release: source.release,
            version,
        })
    }
}
