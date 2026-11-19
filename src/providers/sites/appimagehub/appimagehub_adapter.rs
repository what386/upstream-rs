use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use anyhow::{Context, Result, anyhow, ensure};
use chrono::{DateTime, Utc};
use reqwest::Url;

use crate::{
    models::{
        common::{Version, enums::Filetype},
        provider::{Asset, Release, RepositorySearchFilters, RepositorySearchResult},
    },
    providers::{
        assets::filenames::parser::{parse_arch, parse_filetype},
        sites::ReleaseProvider,
    },
    utils::platform::platform_info::{CpuArch, OSKind},
};

use super::{appimagehub_client::AppImageHubClient, appimagehub_dtos::AppImageHubContentDto};

#[derive(Debug, Clone)]
pub struct AppImageHubAdapter {
    client: AppImageHubClient,
}

impl AppImageHubAdapter {
    pub fn new(client: AppImageHubClient) -> Self {
        Self { client }
    }

    async fn fetch_releases(&self, slug: &str, archived: bool) -> Result<Vec<Release>> {
        Self::convert_content(self.client.get_content(slug, archived).await?)
    }

    fn convert_content(content: AppImageHubContentDto) -> Result<Vec<Release>> {
        let published_at = [content.changed.as_str(), content.created.as_str()]
            .into_iter()
            .find_map(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|date| date.with_timezone(&Utc))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);

        let mut releases = BTreeMap::<String, Release>::new();
        for file in content.files {
            let name = file.name;
            let package_type = file.package_type;
            let parsed_type = parse_filetype(&name);
            let filetype = if package_type.eq_ignore_ascii_case("appimage")
                || parsed_type == Filetype::AppImage
            {
                Filetype::AppImage
            } else if parsed_type == Filetype::Checksum {
                Filetype::Checksum
            } else {
                continue;
            };

            ensure!(
                !name.is_empty()
                    && !matches!(name.as_str(), "." | "..")
                    && !name.contains(['/', '\\', '\0']),
                "Invalid AppImageHub filename"
            );

            let download_url = file.download_url;
            let url = Url::parse(&download_url).context("Invalid AppImageHub download URL")?;
            ensure!(
                matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
                "AppImageHub download URL must use HTTP(S)"
            );

            let file_version = file.version;
            let content_version = content.version.clone();
            let mut tag = if !file_version.is_empty() {
                file_version
            } else if let Ok(version) = Version::from_filename(&name) {
                if Version::from_tag(&content_version).ok().as_ref() == Some(&version) {
                    content_version
                } else {
                    version.to_string()
                }
            } else {
                content_version
            };

            let mut version = Version::from_tag(&tag)
                .or_else(|_| Version::from_filename(&name))
                .unwrap_or(Version::Unknown);

            if version.is_unknown() {
                // A rolling label must change with the listing so installed-tag
                // equality does not suppress updates forever.
                tag = format!(
                    "{}@{}",
                    if tag.is_empty() { "updated" } else { &tag },
                    published_at.timestamp()
                );
            } else if tag.is_empty() {
                tag = version.to_string();
            }

            let lowered_tag = tag.to_ascii_lowercase();
            let is_prerelease = ["alpha", "beta", "preview", "-rc", ".rc"]
                .iter()
                .any(|marker| lowered_tag.contains(marker));

            if let Version::Semver {
                is_prerelease: flag,
                ..
            } = &mut version
            {
                *flag = is_prerelease;
            }

            let mut asset = Asset::with_filetype(
                download_url,
                file.id,
                name,
                file.size_kib.saturating_mul(1024),
                published_at,
                filetype,
            );

            if asset.filetype == Filetype::AppImage {
                asset.target_os = Some(OSKind::Linux);
                let arch = file.architecture;
                asset.target_arch = match arch.to_ascii_lowercase().as_str() {
                    "" | "all" | "any" => asset.target_arch,
                    "i386" | "i486" | "i586" | "i686" => Some(CpuArch::X86),
                    "armhf" | "armel" => Some(CpuArch::Arm),
                    "ppc" | "powerpc" => Some(CpuArch::Ppc),
                    "ppc64" | "powerpc64" => Some(CpuArch::Ppc64),
                    "riscv64" => Some(CpuArch::Riscv64),
                    "s390x" => Some(CpuArch::S390x),
                    value => parse_arch(value)
                        .or(asset.target_arch)
                        .or(Some(CpuArch::Unknown)),
                };
            }

            let release = releases.entry(tag.clone()).or_insert_with(|| Release {
                id: content.id.parse().unwrap_or(0),
                tag,
                name: content.name.clone(),
                body: String::new(), // A store description is not a release changelog.
                is_draft: false,
                is_prerelease,
                assets: Vec::new(),
                version,
                published_at,
            });

            release.assets.push(asset);
        }

        let mut releases = releases
            .into_values()
            .filter(|r| r.assets.iter().any(|a| a.filetype == Filetype::AppImage))
            .collect::<Vec<_>>();

        releases.sort_by(|a, b| b.cmp_version_then_published(a));
        Ok(releases)
    }
}

#[async_trait::async_trait(?Send)]
impl ReleaseProvider for AppImageHubAdapter {
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        self.fetch_releases(slug, false)
            .await?
            .into_iter()
            .find(|release| !release.is_prerelease && !release.tag.to_ascii_lowercase().contains("nightly"))
            .ok_or_else(|| anyhow!("No stable direct AppImage files available for AppImageHub listing {slug}; directory-only links are not supported"))
    }

    async fn get_releases(
        &self,
        slug: &str,
        _per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        let mut releases = self.fetch_releases(slug, false).await?;
        ensure!(
            !releases.is_empty(),
            "AppImageHub listing {slug} has no direct AppImage files; directory-only links and other package formats are not supported"
        );

        if let Some(limit) = max_total {
            releases.truncate(limit as usize);
        }

        Ok(releases)
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        if let Some(release) = self
            .fetch_releases(slug, false)
            .await?
            .into_iter()
            .find(|r| r.tag == tag)
        {
            return Ok(release);
        }

        // Archived files are fetched only for explicit version selection/reinstallation.
        self.fetch_releases(slug, true)
            .await?
            .into_iter()
            .find(|r| r.tag == tag)
            .ok_or_else(|| {
                anyhow!("AppImageHub listing {slug} has no AppImage release tagged '{tag}'")
            })
    }

    async fn search_repositories(
        &self,
        query: &str,
        limit: Option<u32>,
        filters: &RepositorySearchFilters,
    ) -> Result<Vec<RepositorySearchResult>> {
        ensure!(
            *filters == RepositorySearchFilters::default(),
            "AppImageHub search supports text queries and --limit; forge filters (language, topic, stars, pushed-after, forks, archived) are not supported"
        );

        let limit = limit.unwrap_or(10).max(1);
        let page_size = limit.clamp(10, 100);
        let categories = self.client.get_application_categories().await?;
        let mut results = Vec::new();
        let mut seen = HashSet::new();
        let mut page = 0;
        loop {
            crate::application::cancellation::check()?;
            let response = self
                .client
                .search_content(query, &categories, page, page_size)
                .await?;

            let previous_count = seen.len();
            for content in response.contents {
                if !seen.insert(content.id.clone())
                    || !content.files.iter().any(|file| file.is_appimage())
                {
                    continue;
                }

                let updated_at = [content.changed.as_str(), content.created.as_str()]
                    .into_iter()
                    .find_map(|value| DateTime::parse_from_rfc3339(value).ok())
                    .map(|date| date.with_timezone(&Utc))
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);

                let description = if content.description.is_empty() {
                    &content.summary
                } else {
                    &content.description
                };

                let summary = scraper::Html::parse_fragment(description);
                let description = summary
                    .root_element()
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");

                results.push(RepositorySearchResult {
                    repo_slug: content.id,
                    display_name: content.name,
                    description,
                    metadata: Vec::new(),
                    updated_at,
                });
                if results.len() >= limit as usize {
                    return Ok(results);
                }
            }

            if seen.len() == previous_count || seen.len() as u64 >= response.total_items {
                break;
            }

            page = page
                .checked_add(1)
                .context("AppImageHub search page limit reached")?;
        }

        Ok(results)
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        let mut callback = dl_callback;
        self.client
            .download_file(&asset.download_url, destination_path, &mut callback)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{
            common::enums::{Channel, Provider},
            upstream::Package,
        },
        providers::provider_manager::ProviderManager,
    };
    use serde_json::{Value, json};

    fn response(base: &str, version: &str, token: &str) -> Value {
        json!({
            "status": "ok", "statuscode": 100, "message": "", "data": [{
                "id": 2259793, "name": "Example AppImage", "version": version,
                "changed": "2026-01-02T03:04:05+00:00", "created": "2020-01-01T00:00:00+00:00",
                "description": "Store description, not release notes",
                "downloadlink1": format!("{base}/download/{token}"), "downloadname1": "Example.AppImage",
                "download_version1": version, "download_package_type1": "appimage",
                "download_package_arch1": "x86-64", "downloadsize1": 12,
                "downloadlink2": format!("{base}/download/arm"), "downloadname2": "Example-arm.AppImage",
                "download_version2": version, "download_package_type2": "appimage",
                "download_package_arch2": "arm64", "downloadsize2": "10",
                "downloadlink3": format!("{base}/download/old"), "downloadname3": "Example-old.AppImage",
                "download_version3": "v0.9.0", "download_package_type3": "appimage",
                "download_package_arch3": "x86-64",
                "downloadlink4": format!("{base}/download/deb"), "downloadname4": "Example.deb",
                "download_package_type4": "debian-package",
                "downloadlink6": format!("{base}/download/sums"), "downloadname6": "SHA256SUMS.sha256",
                "download_version6": version
            }]
        })
    }

    fn releases_from_response(response: &Value, id: &str) -> Result<Vec<Release>> {
        AppImageHubAdapter::convert_content(AppImageHubContentDto::from_response(response, id)?)
    }

    #[test]
    fn groups_file_versions_and_uses_api_architecture_and_sizes() {
        let releases = releases_from_response(
            &response("https://example.test", "v1.2.3", "fresh"),
            "2259793",
        )
        .unwrap();

        assert_eq!(releases.len(), 2);
        assert_eq!(releases[0].tag, "v1.2.3");
        assert_eq!(releases[0].version, Version::new(1, 2, 3, false));
        assert_eq!(releases[0].assets.len(), 3);
        assert_eq!(releases[0].assets[0].size, 12 * 1024);
        assert_eq!(releases[0].assets[0].target_os, Some(OSKind::Linux));
        assert_eq!(releases[0].assets[0].target_arch, Some(CpuArch::X86_64));
        assert_eq!(releases[0].assets[1].target_arch, Some(CpuArch::Aarch64));
        assert_eq!(releases[0].assets[2].filetype, Filetype::Checksum);
        assert!(releases[0].body.is_empty());
        assert_eq!(releases[1].tag, "v0.9.0");
        assert_eq!(releases[1].assets.len(), 1);
    }

    #[test]
    fn file_metadata_controls_version_and_architecture_and_preview_channel() {
        let mut value = response("https://example.test", "v2.0.0-beta.1", "token");
        value["data"][0]["download_package_arch1"] = json!("unsupported-cpu");
        value["data"][0]["download_version3"] = json!("");
        value["data"][0]["downloadname3"] = json!("Example-0.8.0.AppImage");
        let releases = releases_from_response(&value, "2259793").unwrap();
        assert!(releases[0].is_prerelease);
        assert!(ProviderManager::release_matches_channel(
            &releases[0],
            &Channel::Preview
        ));

        assert!(!ProviderManager::release_matches_channel(
            &releases[0],
            &Channel::Stable
        ));

        assert_eq!(releases[0].assets[0].target_arch, Some(CpuArch::Unknown));
        assert_eq!(releases[1].version, Version::new(0, 8, 0, false));
    }

    #[test]
    fn rejects_unsafe_files_and_skips_directory_links() {
        let value = json!({"status":"ok", "statuscode":100, "data":[{
            "id":2259793, "downloadname1":"downloads", "downloadlink1":"https://download.kde.org/stable/digikam/"
        }]});

        assert!(
            releases_from_response(&value, "2259793")
                .unwrap()
                .is_empty()
        );

        for bad in [
            "../Example.AppImage",
            "..\\Example.AppImage",
            "bad\0.AppImage",
        ] {
            let mut value = response("https://example.test", "1.0.0", "token");
            value["data"][0]["downloadname1"] = json!(bad);
            assert!(releases_from_response(&value, "2259793").is_err());
        }

        let mut value = response("https://example.test", "1.0.0", "token");
        value["data"][0]["downloadlink1"] = json!("file:///tmp/Example.AppImage");
        assert!(releases_from_response(&value, "2259793").is_err());
    }

    #[test]
    fn empty_versions_and_missing_dates_are_deterministic() {
        let mut value = response("https://example.test", "", "token");
        value["data"][0]["changed"] = json!("");
        value["data"][0]["created"] = json!("");
        let releases = releases_from_response(&value, "2259793").unwrap();
        assert!(
            releases
                .iter()
                .all(|r| r.published_at == DateTime::<Utc>::MIN_UTC)
        );

        let release = releases.iter().find(|r| r.version.is_unknown()).unwrap();
        assert!(release.tag.starts_with("updated@"));
    }

    #[test]
    fn rolling_labels_detect_updates_by_listing_timestamp() {
        let mut value = response("https://example.test", "continuous", "token");
        let initial = releases_from_response(&value, "2259793")
            .unwrap()
            .into_iter()
            .find(|r| r.version.is_unknown())
            .unwrap();

        let mut package = Package::with_defaults(
            "appimagehub:2259793".into(),
            "2259793".into(),
            Filetype::AppImage,
            None,
            None,
            Channel::Stable,
            Provider::AppImageHub,
            None,
        );

        package.record_release(&initial);
        assert!(!package.is_update_available(&initial));
        value["data"][0]["changed"] = json!("2026-02-02T03:04:05Z");
        let updated = releases_from_response(&value, "2259793")
            .unwrap()
            .into_iter()
            .find(|r| r.version.is_unknown())
            .unwrap();

        assert!(package.is_update_available(&updated));
    }
}
