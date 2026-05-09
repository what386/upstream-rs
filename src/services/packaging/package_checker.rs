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
        if let Some(branch) = package.build_branch.as_deref() {
            let head = self
                .provider_manager
                .get_branch_head_sha(
                    &package.repo_slug,
                    &package.provider,
                    branch,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch branch head for '{}' on '{}'",
                    branch, package.name
                ))?;

            let current = package
                .build_commit
                .as_deref()
                .map(|c| format!("branch:{}@{}", branch, c))
                .unwrap_or_else(|| format!("branch:{}@unknown", branch));
            let latest = format!("branch:{}@{}", branch, head);

            if package
                .build_commit
                .as_deref()
                .is_some_and(|saved| saved == head)
            {
                return Ok(None);
            }

            return Ok(Some((current, latest)));
        }

        let Some(latest_release) = self
            .provider_manager
            .check_for_updates(package)
            .await
            .context(format!(
                "Failed to fetch latest release for '{}'",
                package.name
            ))?
        else {
            return Ok(None);
        };

        let up_to_date = if package.channel == crate::models::common::enums::Channel::Nightly {
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
