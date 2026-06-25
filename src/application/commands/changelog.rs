use anyhow::{Context, Result, anyhow};

use crate::{
    application::context::CommandContext,
    models::{
        common::{enums::Channel, version::Version},
        provider::Release,
        upstream::Package,
    },
    output,
    output::pager,
    providers::provider_manager::ProviderManager,
};

pub async fn run(name: String, from_tag: Option<String>, to_tag: Option<String>) -> Result<()> {
    let context = CommandContext::new()?;
    let package_database = context.package_database()?;
    let package = package_database
        .get_package(&name)?
        .ok_or_else(|| anyhow!("Package '{}' is not installed", name))?;

    let from_version = match from_tag.as_deref() {
        Some(tag) if is_changelog_endpoint(tag, ChangelogEndpoint::Current) => {
            package.version.clone()
        }
        Some(tag) if is_changelog_endpoint(tag, ChangelogEndpoint::Latest) => {
            context
                .provider_manager
                .get_latest_release(
                    &package.repo_slug,
                    &package.provider,
                    &package.channel,
                    package.base_url.as_deref(),
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to fetch latest {} release for '{}'",
                        package.channel, package.repo_slug
                    )
                })?
                .version
        }
        Some(tag) => {
            context
                .provider_manager
                .get_release_by_tag(
                    &package.repo_slug,
                    tag,
                    &package.provider,
                    package.base_url.as_deref(),
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to fetch starting release '{}' for '{}'",
                        tag, package.repo_slug
                    )
                })?
                .version
        }
        None => package.version.clone(),
    };

    let to_release = match to_tag.as_deref() {
        Some(tag) if is_changelog_endpoint(tag, ChangelogEndpoint::Current) => {
            current_package_release(&package)
        }
        Some(tag) if is_changelog_endpoint(tag, ChangelogEndpoint::Latest) => context
            .provider_manager
            .get_latest_release(
                &package.repo_slug,
                &package.provider,
                &package.channel,
                package.base_url.as_deref(),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch latest {} release for '{}'",
                    package.channel, package.repo_slug
                )
            })?,
        Some(tag) => context
            .provider_manager
            .get_release_by_tag(
                &package.repo_slug,
                tag,
                &package.provider,
                package.base_url.as_deref(),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch ending release '{}' for '{}'",
                    tag, package.repo_slug
                )
            })?,
        None => context
            .provider_manager
            .get_latest_release(
                &package.repo_slug,
                &package.provider,
                &package.channel,
                package.base_url.as_deref(),
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch latest {} release for '{}'",
                    package.channel, package.repo_slug
                )
            })?,
    };

    let Some(changelog) = changelog_text_for_package(
        &context.provider_manager,
        &package,
        &from_version,
        &to_release,
        explicit_to_endpoint(to_tag.as_deref()),
    )
    .await?
    else {
        println!(
            "{}",
            output::warning(format!(
                "No release notes found for '{}' from {} to {}.",
                package.name, from_version, to_release.version
            ))
        );
        return Ok(());
    };

    let renderer = output::MarkdownRenderer::for_terminal();
    let changelog = renderer.render(&changelog);
    pager::page_text(Some(&format!("Changelog: {}", package.name)), &changelog)?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangelogEndpoint {
    Current,
    Latest,
}

fn is_changelog_endpoint(raw: &str, endpoint: ChangelogEndpoint) -> bool {
    match endpoint {
        ChangelogEndpoint::Current => raw.eq_ignore_ascii_case("current"),
        ChangelogEndpoint::Latest => raw.eq_ignore_ascii_case("latest"),
    }
}

fn explicit_to_endpoint(raw: Option<&str>) -> bool {
    raw.is_some_and(|tag| !is_changelog_endpoint(tag, ChangelogEndpoint::Latest))
}

fn current_package_release(package: &Package) -> Release {
    Release {
        id: 0,
        tag: package.version.to_string(),
        name: format!("current ({})", package.version),
        body: String::new(),
        is_draft: false,
        is_prerelease: package.version.is_prerelease,
        assets: Vec::new(),
        version: package.version.clone(),
        published_at: package.last_upgraded,
    }
}

pub async fn changelog_text_for_package(
    provider_manager: &ProviderManager,
    package: &Package,
    from_version: &Version,
    to_release: &Release,
    explicit_to: bool,
) -> Result<Option<String>> {
    let releases = provider_manager
        .get_releases_newer_than(
            &package.repo_slug,
            &package.provider,
            from_version,
            None,
            package.base_url.as_deref(),
        )
        .await
        .with_context(|| format!("Failed to fetch releases for '{}'", package.repo_slug))?;

    let releases =
        select_changelog_releases(releases, package, from_version, to_release, explicit_to);

    Ok(changelog_text_from_releases(
        package,
        from_version,
        to_release,
        &releases,
    ))
}

pub fn changelog_text_from_releases(
    package: &Package,
    from_version: &Version,
    to_release: &Release,
    releases: &[Release],
) -> Option<String> {
    if releases.is_empty() {
        return None;
    }

    let mut changelog = String::new();
    changelog.push_str(&format!(
        "  Range:        {} -> {}\n",
        from_version, to_release.version
    ));
    changelog.push_str(&format!(
        "  Source:       {} ({})\n",
        package.repo_slug, package.provider
    ));
    changelog.push_str(&format!("  Channel:      {}\n\n", package.channel));

    for release in releases {
        changelog.push_str(&format!("## {}\n", release_heading(release)));
        changelog.push_str(&format!(
            "tag {} - published {}\n\n",
            release.tag,
            release.published_at.format("%Y-%m-%d")
        ));
        if release.body.trim().is_empty() {
            changelog.push_str("(no release notes)\n");
        } else {
            changelog.push_str(release.body.trim());
            changelog.push('\n');
        }
        changelog.push('\n');
    }

    Some(changelog)
}

fn select_changelog_releases(
    releases: Vec<Release>,
    package: &Package,
    from_version: &Version,
    to_release: &Release,
    explicit_to: bool,
) -> Vec<Release> {
    let mut selected: Vec<Release> = releases
        .into_iter()
        .filter(|release| !release.is_draft)
        .filter(|release| explicit_to || release_matches_channel(release, &package.channel))
        .filter(|release| release.version > *from_version && release.version <= to_release.version)
        .collect();

    if !selected
        .iter()
        .any(|release| release.tag.eq_ignore_ascii_case(&to_release.tag))
        && to_release.version > *from_version
        && (explicit_to || release_matches_channel(to_release, &package.channel))
    {
        selected.push(to_release.clone());
    }

    selected.sort_by(|a, b| a.version.cmp(&b.version));
    selected.dedup_by(|a, b| a.tag.eq_ignore_ascii_case(&b.tag));
    selected
}

fn release_matches_channel(release: &Release, channel: &Channel) -> bool {
    match channel {
        Channel::Stable => {
            !release.is_prerelease && !ProviderManager::is_nightly_release(&release.tag)
        }
        Channel::Preview => ProviderManager::is_preview_release(release),
        Channel::Nightly => ProviderManager::is_nightly_release(&release.tag),
    }
}

fn release_heading(release: &Release) -> String {
    if release.name.trim().is_empty() || release.name == release.tag {
        release.tag.clone()
    } else {
        format!("{} ({})", release.name, release.tag)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChangelogEndpoint, current_package_release, explicit_to_endpoint, is_changelog_endpoint,
        select_changelog_releases,
    };
    use crate::models::{
        common::{
            enums::{Channel, Filetype, Provider},
            version::Version,
        },
        provider::Release,
        upstream::Package,
    };
    use chrono::Utc;

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            id: 1,
            tag: tag.to_string(),
            name: tag.to_string(),
            body: format!("notes for {tag}"),
            is_draft: false,
            is_prerelease: prerelease,
            assets: Vec::new(),
            version: Version::from_tag(tag).expect("version tag"),
            published_at: Utc::now(),
        }
    }

    fn package(channel: Channel) -> Package {
        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            channel,
            Provider::Github,
            None,
        );
        package.version = Version::new(1, 0, 0, false);
        package
    }

    #[test]
    fn select_changelog_releases_excludes_current_and_includes_latest() {
        let package = package(Channel::Stable);
        let to = release("v1.2.0", false);
        let selected = select_changelog_releases(
            vec![
                release("v1.0.0", false),
                release("v1.1.0", false),
                to.clone(),
            ],
            &package,
            &package.version,
            &to,
            false,
        );

        let tags = selected
            .iter()
            .map(|release| release.tag.as_str())
            .collect::<Vec<_>>();
        assert_eq!(tags, vec!["v1.1.0", "v1.2.0"]);
    }

    #[test]
    fn select_changelog_releases_filters_preview_for_stable_latest() {
        let package = package(Channel::Stable);
        let to = release("v1.2.0", false);
        let selected = select_changelog_releases(
            vec![
                release("v1.1.0-rc1", true),
                release("v1.1.0", false),
                to.clone(),
            ],
            &package,
            &package.version,
            &to,
            false,
        );

        let tags = selected
            .iter()
            .map(|release| release.tag.as_str())
            .collect::<Vec<_>>();
        assert_eq!(tags, vec!["v1.1.0", "v1.2.0"]);
    }

    #[test]
    fn select_changelog_releases_allows_explicit_to_outside_channel() {
        let package = package(Channel::Stable);
        let to = release("v1.1.0-rc1", true);
        let selected =
            select_changelog_releases(vec![to.clone()], &package, &package.version, &to, true);

        assert_eq!(selected[0].tag, "v1.1.0-rc1");
    }

    #[test]
    fn changelog_endpoint_keywords_are_case_insensitive() {
        assert!(is_changelog_endpoint("current", ChangelogEndpoint::Current));
        assert!(is_changelog_endpoint("CURRENT", ChangelogEndpoint::Current));
        assert!(is_changelog_endpoint("latest", ChangelogEndpoint::Latest));
        assert!(is_changelog_endpoint("Latest", ChangelogEndpoint::Latest));
        assert!(!is_changelog_endpoint("v1.2.3", ChangelogEndpoint::Latest));
    }

    #[test]
    fn latest_to_endpoint_keeps_default_channel_filtering() {
        assert!(!explicit_to_endpoint(None));
        assert!(!explicit_to_endpoint(Some("latest")));
        assert!(explicit_to_endpoint(Some("current")));
        assert!(explicit_to_endpoint(Some("v1.2.3")));
    }

    #[test]
    fn current_package_release_uses_installed_version_as_endpoint() {
        let package = package(Channel::Stable);
        let release = current_package_release(&package);

        assert_eq!(release.version, package.version);
        assert_eq!(release.tag, package.version.to_string());
        assert_eq!(release.published_at, package.last_upgraded);
    }
}
