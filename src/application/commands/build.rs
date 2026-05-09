use anyhow::Result;

use crate::application::cli::arguments::BuildProfile as CliBuildProfile;
use crate::application::operations::build_operation::{BuildCommandInput, BuildOperation};
use crate::models::common::enums::{Channel, Provider};
use crate::providers::provider_manager::ProviderManager;
use crate::services::builder::BuildProfile;
use crate::services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage};
use crate::utils::static_paths::UpstreamPaths;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: String,
    repo_slug: String,
    tag: Option<String>,
    branch: Option<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    desktop: bool,
    yes: bool,
    build_profile: Option<CliBuildProfile>,
    build_output: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let _ = yes;
    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let app_config = config.get_config();

    let github_token = app_config.github.api_token.as_deref();
    let gitlab_token = app_config.gitlab.api_token.as_deref();
    let gitea_token = app_config.gitea.api_token.as_deref();
    let trusted_keys = app_config.trusted_signature_keys();

    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;
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
            build_output,
            dry_run,
        })
        .await
}
