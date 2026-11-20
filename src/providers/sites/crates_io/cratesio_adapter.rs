use super::{
    CratesIoClient, CratesIoIndexClient,
    cratesio_dtos::{IndexVersion, SearchResponse},
};
use crate::{
    models::{
        common::{Version, enums::Channel},
        provider::{
            Asset, Release, RepositorySearchFilters, RepositorySearchResult, SearchMetadataValue,
        },
    },
    providers::sites::ReleaseProvider,
};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use jiff::Timestamp;
use std::path::Path;

#[derive(Clone)]
pub struct CratesIoAdapter {
    client: CratesIoClient,
    index: CratesIoIndexClient,
}

pub fn crate_name(source: &str) -> Result<String> {
    let source = source
        .trim()
        .strip_prefix("crates-io:")
        .unwrap_or(source.trim());

    let name = if source.starts_with("https://") {
        let url = reqwest::Url::parse(source)?;
        ensure!(
            url.host_str() == Some("crates.io"),
            "Expected a crates.io URL"
        );

        let parts = url.path().trim_matches('/').split('/').collect::<Vec<_>>();
        ensure!(
            parts.len() == 2 && parts[0] == "crates",
            "Expected https://crates.io/crates/<name>"
        );

        parts[1].to_owned()
    } else {
        source.to_owned()
    };

    ensure!(
        !name.is_empty()
            && name.len() <= 64
            && name.as_bytes()[0].is_ascii_alphabetic()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "Invalid crate name '{name}'"
    );

    Ok(name)
}

impl CratesIoAdapter {
    pub fn new(client: CratesIoClient, index: CratesIoIndexClient) -> Self {
        Self { client, index }
    }

    async fn version(&self, name: &str, version: &str) -> Result<IndexVersion> {
        Version::parse_strict_semver(version)?;
        let versions = self.index.versions(name).await?;
        let found = versions
            .iter()
            .find(|v| v.num == version)
            .ok_or_else(|| anyhow::anyhow!("Version {version} not found for '{name}'"))?;

        ensure!(!found.yanked, "Crate {name}@{version} has been yanked");
        Ok(found.clone())
    }

    pub(crate) async fn latest(&self, name: &str, channel: &Channel) -> Result<Release> {
        let versions = self.index.versions(name).await?;
        versions
            .iter()
            .filter(|v| !v.yanked)
            .map(IndexVersion::release)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|r| match channel {
                Channel::Stable => !r.is_prerelease,
                Channel::Preview => r.is_prerelease,
                Channel::Nightly => false,
            })
            .max_by(Release::cmp_version_then_published)
            .ok_or_else(|| anyhow::anyhow!("No non-yanked {channel} version found for '{name}'"))
    }
}

#[async_trait(?Send)]
impl ReleaseProvider for CratesIoAdapter {
    async fn get_source_archive(&self, name: &str, version: &str) -> Result<Asset> {
        let name = crate_name(name)?;
        let entry = self.version(&name, version).await?;
        let name = &entry.name;
        tracing::debug!(crate_name = %name, minimum_rust = ?entry.rust_version, "Resolved published crate source");
        let mut asset = Asset::new(
            format!("https://static.crates.io/crates/{name}/{name}-{version}.crate"),
            0,
            format!("{name}-{version}.tar.gz"),
            0,
            entry.pubtime.unwrap_or(Timestamp::MIN),
        );

        asset.expected_sha256 = Some(entry.checksum);
        Ok(asset)
    }

    async fn get_latest_release(&self, name: &str) -> Result<Release> {
        self.latest(name, &Channel::Stable).await
    }

    async fn get_releases(
        &self,
        name: &str,
        _: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        let mut releases = self
            .index
            .versions(name)
            .await?
            .iter()
            .filter(|v| !v.yanked)
            .map(IndexVersion::release)
            .collect::<Result<Vec<_>>>()?;

        releases.sort_by(|a, b| b.cmp_version_then_published(a));
        if let Some(max) = max_total {
            releases.truncate(max as usize);
        }

        Ok(releases)
    }

    async fn get_release_by_tag(&self, name: &str, version: &str) -> Result<Release> {
        self.version(name, version).await?.release()
    }

    async fn search_repositories(
        &self,
        query: &str,
        limit: Option<u32>,
        _: &RepositorySearchFilters,
    ) -> Result<Vec<RepositorySearchResult>> {
        let limit = limit.unwrap_or(10).max(1) as usize;
        let per_page = limit.min(100).to_string();
        let mut results = Vec::new();
        let mut page = 1;
        while results.len() < limit {
            let response: SearchResponse = self
                .client
                .get(
                    "crates",
                    &[
                        ("q", query),
                        ("page", &page.to_string()),
                        ("per_page", &per_page),
                    ],
                )
                .await?;

            if response.crates.is_empty() {
                break;
            }

            results.extend(response.crates.into_iter().map(|c| RepositorySearchResult {
                repo_slug: c.name.clone(),
                display_name: c.name,
                description: c.description.unwrap_or_default(),
                updated_at: c.updated_at,
                metadata: vec![
                    (
                        "downloads".into(),
                        SearchMetadataValue::Integer(c.downloads),
                    ),
                    ("version".into(), SearchMetadataValue::Text(c.max_version)),
                ],
            }));
            if results.len() >= response.meta.total {
                break;
            }

            page += 1;
        }

        results.truncate(limit);
        Ok(results)
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination: &Path,
        callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        self.client
            .download(
                &asset.download_url,
                destination,
                &mut callback.map(|cb| cb as &mut dyn FnMut(u64, u64)),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::crate_name;

    #[test]
    fn normalizes_crate_sources_and_rejects_non_crate_paths() {
        for source in [
            "ripgrep",
            "crates-io:ripgrep",
            "https://crates.io/crates/ripgrep/",
        ] {
            assert_eq!(crate_name(source).unwrap(), "ripgrep");
        }

        for source in [
            "",
            "../ripgrep",
            "owner/repo",
            "https://example.com/crates/ripgrep",
            "https://crates.io/crates/ripgrep/versions",
        ] {
            assert!(crate_name(source).is_err(), "{source}");
        }
    }
}
