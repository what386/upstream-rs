use anyhow::{Context, Result, anyhow};

use crate::{
    models::{
        common::{enums::Channel, version::Version},
        provider::Release,
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};

pub async fn run(name: String, from_tag: Option<String>, to_tag: Option<String>) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let package = package_storage
        .get_package_by_name(&name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed", name))?;

    let app_config = config.get_config();
    let provider_manager = ProviderManager::new(
        app_config.github.api_token.as_deref(),
        app_config.gitlab.api_token.as_deref(),
        app_config.gitea.api_token.as_deref(),
    )?;

    let from_version = match from_tag.as_deref() {
        Some(tag) => {
            provider_manager
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
        Some(tag) => provider_manager
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
        None => provider_manager
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

    let releases = provider_manager
        .get_releases(
            &package.repo_slug,
            &package.provider,
            None,
            None,
            package.base_url.as_deref(),
        )
        .await
        .with_context(|| format!("Failed to fetch releases for '{}'", package.repo_slug))?;

    let releases = select_changelog_releases(
        releases,
        package,
        &from_version,
        &to_release,
        to_tag.is_some(),
    );

    if releases.is_empty() {
        println!(
            "No release notes found for '{}' from {} to {}.",
            package.name, from_version, to_release.version
        );
        return Ok(());
    }

    for (index, release) in releases.iter().enumerate() {
        if releases.len() > 1 {
            if index > 0 {
                println!();
            }
            println!("## {}", release_heading(release));
        }

        if release.body.trim().is_empty() {
            println!("(no release notes)");
        } else {
            println!("{}", release.body.trim());
        }
    }

    Ok(())
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
    use super::select_changelog_releases;
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
}
