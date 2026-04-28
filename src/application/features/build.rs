use anyhow::Result;

use crate::application::cli::arguments::BuildProfile as CliBuildProfile;
use crate::application::operations::build_operation::{BuildCommandInput, BuildOperation};
use crate::models::common::enums::{Channel, Provider};
use crate::services::builder::BuildProfile;
use crate::services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage};
use crate::utils::static_paths::UpstreamPaths;
use crate::{providers::provider_manager::ProviderManager};

#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: String,
    repo_slug: String,
    tag: Option<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    match_pattern: Option<String>,
    exclude_pattern: Option<String>,
    desktop: bool,
    yes: bool,
    build_profile: CliBuildProfile,
    build_output: Option<String>,
) -> Result<()> {
    let _ = (match_pattern, exclude_pattern, yes);
    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let github_token = config.get_config().github.api_token.as_deref();
    let gitlab_token = config.get_config().gitlab.api_token.as_deref();
    let gitea_token = config.get_config().gitea.api_token.as_deref();

    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;
    let mut operation = BuildOperation::new(&provider_manager, &mut package_storage, &paths);

    operation
        .build_and_install(BuildCommandInput {
            name,
            repo_slug,
            tag,
            provider,
            base_url,
            channel,
            desktop,
            build_profile: match build_profile {
                CliBuildProfile::Rust => BuildProfile::Rust,
                CliBuildProfile::Dotnet => BuildProfile::Dotnet,
            },
            build_output,
        })
        .await
}
