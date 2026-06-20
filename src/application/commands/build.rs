use anyhow::Result;

use crate::application::cli::arguments::BuildProfile as CliBuildProfile;
use crate::application::context::CommandContext;
use crate::application::operations::build_operation::{BuildCommandInput, BuildOperation};
use crate::models::common::enums::{Channel, Provider};
use crate::providers::discovery::infer_package_name;
use crate::services::builder::BuildProfile;

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
    let context = CommandContext::new()?;
    let mut package_storage = context.package_storage()?;
    let name = resolve_package_name(name, &repo_slug, provider.as_ref(), base_url.as_deref())?;
    let mut operation = BuildOperation::new(
        &context.provider_manager,
        &mut package_storage,
        &context.paths,
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
