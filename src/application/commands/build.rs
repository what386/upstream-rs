use anyhow::Result;

use crate::application::cli::arguments::BuildProfile as CliBuildProfile;
use crate::application::operations::build_operation::{BuildCommandInput, BuildOperation};
use crate::models::common::enums::{Channel, Provider};
use crate::providers::discovery::infer_package_name;
use crate::providers::provider_manager::ProviderManager;
use crate::services::builder::BuildProfile;
use crate::services::storage::{
    config_storage::ConfigStorage, package_storage::PackageStorage, trust_storage::TrustStorage,
};
use crate::utils::static_paths::UpstreamPaths;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: Option<String>,
    repo_slug: String,
    tag: Option<String>,
    branch: Option<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    desktop: bool,
    build_profile: Option<CliBuildProfile>,
    dry_run: bool,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let trust_storage = TrustStorage::new(&paths.config.trust_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let app_config = config.get_config();

    let github_token = app_config.github.api_token.as_deref();
    let gitlab_token = app_config.gitlab.api_token.as_deref();
    let gitea_token = app_config.gitea.api_token.as_deref();
    let trusted_keys = trust_storage.trusted_signature_keys();

    let provider_manager = ProviderManager::new_with_download_config(
        github_token,
        gitlab_token,
        gitea_token,
        app_config.download,
    )?;
    let name = resolve_package_name(name, &repo_slug, provider.as_ref(), base_url.as_deref())?;
    let mut operation = BuildOperation::new(
        &provider_manager,
        &mut package_storage,
        &paths,
        trusted_keys,
    );

    operation
        .build_and_install(BuildCommandInput {
            name,
            repo_slug,
            tag,
            branch,
            provider,
            base_url,
            channel,
            desktop,
            build_profile: build_profile.map(|profile| match profile {
                CliBuildProfile::Rust => BuildProfile::Rust,
                CliBuildProfile::Dotnet => BuildProfile::Dotnet,
                CliBuildProfile::Go => BuildProfile::Go,
                CliBuildProfile::Zig => BuildProfile::Zig,
                CliBuildProfile::Cmake => BuildProfile::Cmake,
            }),
            dry_run,
        })
        .await
}

fn resolve_package_name(
    name: Option<String>,
    source: &str,
    provider: Option<&Provider>,
    base_url: Option<&str>,
) -> Result<String> {
    if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
        return Ok(name);
    }

    infer_package_name(source, provider, base_url)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Package name is required for this source. Provide a name after the repository or URL."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_package_name;
    use crate::models::common::enums::Provider;

    #[test]
    fn resolve_package_name_infers_git_repo_name_when_omitted() {
        assert_eq!(
            resolve_package_name(None, "BurntSushi/ripgrep", None, None).expect("resolve name"),
            "ripgrep"
        );
        assert_eq!(
            resolve_package_name(
                None,
                "https://codeberg.org/forgejo/forgejo",
                Some(&Provider::Gitea),
                Some("https://codeberg.org"),
            )
            .expect("resolve name"),
            "forgejo"
        );
    }

    #[test]
    fn resolve_package_name_requires_name_for_http_sources() {
        let err = resolve_package_name(None, "https://example.invalid/downloads", None, None)
            .expect_err("name should be required");

        assert!(err.to_string().contains("Package name is required"));
    }
}
