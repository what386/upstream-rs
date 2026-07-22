use anyhow::Result;

use crate::application::context::CommandContext;
use crate::application::operations::build_op::{BuildCommandInput, BuildOperation};
use crate::models::common::enums::{BuildProfile, Channel, Provider};
use crate::models::upstream::{
    BuildInstallSource, BuildSelector, InstallPlan, InstallSource, config::AppConfig,
};
use crate::output;
use crate::providers::discovery::infer_package_name;
use crate::utils::static_paths::UpstreamPaths;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: Option<String>,
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
    let name = resolve_package_name(name, &repo_slug, provider.as_ref(), base_url.as_deref())?;
    let plan = InstallPlan {
        name,
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
        name,
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
            name,
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

fn resolve_package_name(
    name: Option<String>,
    source: &str,
    provider: Option<&Provider>,
    base_url: Option<&str>,
) -> Result<String> {
    if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
        return Ok(name);
    }

    let Some(default) = infer_package_name(source, provider, base_url)? else {
        return Err(anyhow::anyhow!(
            "Package name is required for this source. Provide a name after the repository or URL."
        ));
    };

    output::prompt_text("Package name", Some(&default))
}

#[cfg(test)]
mod tests {
    use crate::models::common::enums::Provider;
    use crate::providers::discovery::infer_package_name;

    #[test]
    fn default_package_name_infers_git_repo_name_when_omitted() {
        assert_eq!(
            default_package_name("BurntSushi/ripgrep", None, None).expect("default name"),
            Some("ripgrep".to_string())
        );
        assert_eq!(
            default_package_name(
                "https://codeberg.org/forgejo/forgejo",
                Some(&Provider::Gitea),
                Some("https://codeberg.org"),
            )
            .expect("default name"),
            Some("forgejo".to_string())
        );
    }

    #[test]
    fn default_package_name_returns_none_for_http_sources() {
        let default =
            default_package_name("https://example.invalid/downloads", None, None).expect("default");

        assert_eq!(default, None);
    }

    fn default_package_name(
        source: &str,
        provider: Option<&Provider>,
        base_url: Option<&str>,
    ) -> anyhow::Result<Option<String>> {
        infer_package_name(source, provider, base_url)
    }
}
