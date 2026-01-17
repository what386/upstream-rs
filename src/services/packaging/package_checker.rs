use crate::{models::upstream::Package, providers::provider_manager::ProviderManager};

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
        let latest = self
            .provider_manager
            .get_latest_release(&package.repo_slug, &package.provider, &package.channel)
            .await
            .context(format!(
                "Failed to fetch latest release for '{}'",
                package.name
            ))?;

        if latest.version.is_newer_than(&package.version) {
            Ok(Some((
                package.version.to_string(),
                latest.version.to_string(),
            )))
        } else {
            Ok(None)
        }
    }

    /// Returns a list of packages with updates available
    pub async fn check_all(&self, packages: &[Package]) -> Result<Vec<(String, String, String)>> {
        let mut updates = Vec::new();

        for pkg in packages {
            if let Some((current, latest)) = self.check_one(pkg).await? {
                updates.push((pkg.name.clone(), current, latest));
            }
        }

        Ok(updates)
    }
}
