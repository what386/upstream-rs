use anyhow::Result;

use crate::application::context::CommandContext;
use crate::application::operations::build_op::{BuildCommandInput, BuildOperation};
use crate::models::common::enums::{BuildProfile, Channel, Provider};
use crate::models::upstream::{
    BuildInstallSource, BuildSelector, InstallPlan, InstallSource, config::AppConfig,
};
use crate::utils::static_paths::UpstreamPaths;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    repo_slug: String,
    tag: Option<String>,
    semver: Option<String>,
    branch: Option<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    desktop: bool,
    build_profile: Option<BuildProfile>,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let plan = InstallPlan {
        desktop,
        source: InstallSource::Build(BuildInstallSource {
            source: repo_slug,
            provider,
            base_url,
            channel,
            selector: BuildSelector::from_options(tag, semver, branch),
            profile: build_profile,
        }),
    };

    run_plan(plan, dry_run, paths, app_config).await
}

pub async fn run_plan(
    plan: InstallPlan,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let InstallPlan {
        desktop,
        source: InstallSource::Build(source),
    } = plan
    else {
        return Err(anyhow::anyhow!(
            "Build command requires a build install plan"
        ));
    };

    let (tag, semver, branch) = source.selector.into_options();
    let context = CommandContext::new(paths, app_config)?;
    let mut package_database = context.package_database()?;
    let mut operation = BuildOperation::new(
        &context.provider_manager,
        &mut package_database,
        context.paths,
    );

    operation
        .build_and_install(BuildCommandInput {
            repo_slug: source.source,
            tag,
            semver,
            branch,
            provider: source.provider,
            base_url: source.base_url,
            channel: source.channel,
            desktop,
            build_profile: source.profile,
            dry_run,
        })
        .await
}
