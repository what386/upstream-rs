use crate::{
    models::upstream::{Package, config::ConcurrencyConfig},
    providers::provider_manager::ProviderManager,
};

use anyhow::{Context, Result};
use futures_util::{StreamExt, stream::FuturesUnordered};

pub struct PackageChecker<'a> {
    provider_manager: &'a ProviderManager,
    concurrency_config: ConcurrencyConfig,
}

impl<'a> PackageChecker<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        concurrency_config: ConcurrencyConfig,
    ) -> Self {
        Self {
            provider_manager,
            concurrency_config,
        }
    }

    /// Returns (current_version, latest_version) if update is available
    pub async fn check_one(&self, package: &Package) -> Result<Option<(String, String)>> {
        // A pinned package is intentionally excluded from upgrade discovery.
        if package.is_pinned {
            return Ok(None);
        }

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
            .context(format!("fetch latest release for '{}'", package.name))?
        else {
            return Ok(None);
        };

        if !package.is_update_available(&latest_release) {
            return Ok(None);
        }

        Ok(Some((
            package.version.to_string(),
            latest_release.version.to_string(),
        )))
    }

    pub async fn check_many(
        &self,
        packages: Vec<Package>,
        checking_callback: &mut dyn FnMut(&str),
    ) -> Vec<(Package, Result<Option<(String, String)>>)> {
        let package_count = packages.len();
        let mut checked = Vec::with_capacity(package_count);
        let mut package_iter = packages.into_iter().enumerate();
        let mut pending = FuturesUnordered::new();

        for _ in 0..self.concurrency_config.check_concurrency() {
            let Some((idx, package)) = package_iter.next() else {
                break;
            };

            checking_callback(&package.name);
            pending.push(self.check_package_at_index(idx, package));
        }

        while let Some((idx, package, result)) = pending.next().await {
            checked.push((idx, package, result));

            if let Some((next_idx, next_package)) = package_iter.next() {
                checking_callback(&next_package.name);
                pending.push(self.check_package_at_index(next_idx, next_package));
            }
        }

        checked.sort_by_key(|(idx, _, _)| *idx);
        checked
            .into_iter()
            .map(|(_, package, result)| (package, result))
            .collect()
    }

    async fn check_package_at_index(
        &self,
        idx: usize,
        package: Package,
    ) -> (usize, Package, Result<Option<(String, String)>>) {
        let result = self.check_one(&package).await;
        (idx, package, result)
    }
}

#[cfg(test)]
mod tests {
    use super::PackageChecker;
    use crate::{
        models::{
            common::enums::{Channel, Filetype, Provider},
            upstream::config::ConcurrencyConfig,
        },
        providers::provider_manager::ProviderManager,
    };

    fn pinned_package(name: &str) -> crate::models::upstream::Package {
        let mut package = crate::models::upstream::Package::with_defaults(
            name.to_string(),
            "owner/tool".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        package.is_pinned = true;
        package
    }

    #[tokio::test]
    async fn check_many_suppresses_pinned_packages_and_preserves_order() {
        let provider_manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");

        let checker = PackageChecker::new(&provider_manager, ConcurrencyConfig::default());
        let packages = vec![pinned_package("first"), pinned_package("second")];
        let mut checked_names = Vec::new();

        let results = checker
            .check_many(packages, &mut |name| checked_names.push(name.to_string()))
            .await;

        assert_eq!(checked_names, ["first", "second"]);
        assert_eq!(
            results
                .iter()
                .map(|(package, _)| package.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );

        assert!(
            results
                .into_iter()
                .all(|(_, result)| result.expect("check package").is_none())
        );
    }
}
