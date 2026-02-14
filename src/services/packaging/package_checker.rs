use crate::{
    models::{common::enums::Channel, upstream::Package},
    providers::provider_manager::ProviderManager,
};

use anyhow::{Context, Result};

pub struct PackageChecker<'a> {
    provider_manager: &'a ProviderManager,
}

impl<'a> PackageChecker<'a> {
    pub fn new(provider_manager: &'a ProviderManager) -> Self {
        Self { provider_manager }
    }

    /// Returns (current_version, latest_version) if update is available
    pub async fn check_one(&self, package: &Package) -> Result<Option<(String, String)>> {
        let Some(latest_release) = self
            .provider_manager
            .get_latest_release_if_modified_since(
                &package.repo_slug,
                &package.provider,
                &package.channel,
                Some(package.last_upgraded),
            )
            .await
            .context(format!(
                "Failed to fetch latest release for '{}'",
                package.name
            ))?
        else {
            return Ok(None);
        };

        let up_to_date = if package.channel == Channel::Nightly {
            latest_release.published_at <= package.last_upgraded
        } else {
            !latest_release.version.is_newer_than(&package.version)
        };

        if up_to_date {
            return Ok(None);
        }

        Ok(Some((
            package.version.to_string(),
            latest_release.version.to_string(),
        )))
    }
}
