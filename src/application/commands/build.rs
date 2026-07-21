use anyhow::Result;

use crate::application::cli::arguments::BuildProfile as CliBuildProfile;
use crate::application::context::CommandContext;
use crate::application::operations::build_op::{BuildCommandInput, BuildOperation};
use crate::models::common::enums::{Channel, Provider};
use crate::output;
use crate::providers::discovery::infer_package_name;
use crate::routines::build::BuildProfile;

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
    build_profile: Option<CliBuildProfile>,
    dry_run: bool,
) -> Result<()> {
    let context = CommandContext::new()?;
    let mut package_database = context.package_database()?;
    let name = resolve_package_name(name, &repo_slug, provider.as_ref(), base_url.as_deref())?;
    let mut operation = BuildOperation::new(
        &context.provider_manager,
        &mut package_database,
        &context.paths,
    );

    operation
        .build_and_install(BuildCommandInput {
            name,
            repo_slug,
            tag,
            semver,
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
