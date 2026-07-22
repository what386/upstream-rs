mod cache;
mod matching;
mod schema;

use anyhow::{Context, Result};

use crate::{
    models::{
        common::enums::{Channel, Filetype},
        upstream::{
            BuildInstallSource, BuildSelector, HttpInstallSource, InstallPlan, InstallSource,
            ReleaseInstallSource, ReleaseSelector,
        },
    },
    utils::static_paths::UpstreamPaths,
};

pub use cache::FetchOutcome;
use cache::{fetch_index, load_cached_index};
use matching::missing_package_message;
use schema::{RegistryInstall, RegistryPackage};

pub async fn resolve(
    name: String,
    fetch: bool,
    paths: &UpstreamPaths,
    index_url: &str,
) -> Result<(InstallPlan, Option<FetchOutcome>)> {
    let cache_file = paths.dirs.cache_dir.join("registry/index.min.json");
    let metadata_file = paths.dirs.cache_dir.join("registry/metadata.json");
    let fetch_outcome = if fetch {
        Some(fetch_index(index_url, &cache_file, &metadata_file).await?)
    } else {
        None
    };

    let index = load_cached_index(&cache_file, &metadata_file, index_url)?;
    let package = index
        .packages
        .get(&name)
        .with_context(|| missing_package_message(&index, &name))?;

    let source = match &package.install {
        RegistryInstall::Release { repo, provider } => {
            InstallSource::Release(ReleaseInstallSource {
                source: repo.clone(),
                kind: Filetype::Auto,
                provider: Some(provider.provider()),
                base_url: None,
                channel: Channel::Stable,
                selector: ReleaseSelector::Latest,
                match_pattern: joined_patterns(&package.r#match),
                exclude_pattern: joined_patterns(&package.exclude),
                trust_mode: package.trust.trust_mode(),
            })
        }
        RegistryInstall::Build {
            repo,
            provider,
            profile,
            branch,
        } => InstallSource::Build(BuildInstallSource {
            source: repo.clone(),
            provider: Some(provider.provider()),
            base_url: None,
            channel: Channel::Stable,
            selector: branch
                .clone()
                .map(BuildSelector::Branch)
                .unwrap_or(BuildSelector::Latest),
            profile: profile.as_ref().map(|profile| profile.build_profile()),
        }),
        RegistryInstall::Http { url, filetype } => InstallSource::Http(HttpInstallSource {
            url: url.clone(),
            kind: filetype.filetype(),
            trust_mode: package.trust.trust_mode(),
        }),
    };

    Ok((
        InstallPlan {
            name: installed_name(&name, package),
            desktop: package.desktop,
            source,
        },
        fetch_outcome,
    ))
}

fn installed_name(registry_name: &str, package: &RegistryPackage) -> String {
    package
        .binary
        .clone()
        .unwrap_or_else(|| registry_name.to_string())
}

fn joined_patterns(patterns: &[String]) -> Option<String> {
    (!patterns.is_empty()).then(|| patterns.join(","))
}

#[cfg(test)]
mod tests {
    use super::{installed_name, joined_patterns};
    use crate::routines::registry::schema::parse_index;

    #[test]
    fn resolves_installed_name_and_patterns() {
        let index = parse_index(
            br#"{"version":1,"packages":{"ripgrep":{"revision":2,"binary":"rg","desktop":false,"trust":"best-effort","match":["linux","x86_64"],"install":{"type":"release","repo":"o/ripgrep","provider":"github"}}}}"#,
        )
        .expect("valid index");
        let package = &index.packages["ripgrep"];
        assert_eq!(installed_name("ripgrep", package), "rg");
        assert_eq!(
            joined_patterns(&package.r#match).as_deref(),
            Some("linux,x86_64")
        );
        assert_eq!(joined_patterns(&package.exclude), None);
    }
}
