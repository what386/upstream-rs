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

                let summary = scraper::Html::parse_fragment(&content.summary);
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
                    // OCS ratings and uploader metadata are not forge stars or source language.
                    stars: 0,
                    language: String::new(),
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
            provider::RepositorySearchFilters,
            upstream::Package,
        },
        providers::provider_manager::ProviderManager,
        utils::test_support,
    };
    use serde_json::{Value, json};
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

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

    #[tokio::test]
    async fn search_filters_and_paginates_appimages_and_preserves_content_ids() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server_base = base.clone();
        let server = thread::spawn(move || {
            for page in [None, Some(0), Some(1)] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap();
                let url = Url::parse(&format!("{server_base}{path}")).unwrap();
                let params = url
                    .query_pairs()
                    .into_owned()
                    .collect::<std::collections::HashMap<_, _>>();

                assert_eq!(params["format"], "json");
                if let Some(page) = page {
                    assert_eq!(url.path(), "/ocs/v1/content/data");
                    assert_eq!(params["search"], "paint & draw");
                    assert_eq!(params["categories"], "239x244");
                    assert_eq!(params["page"], page.to_string());
                    assert_eq!(params["pagesize"], "10");
                } else {
                    assert_eq!(url.path(), "/ocs/v1/content/categories");
                }

                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }

                let first = response(&server_base, "v1.0.0", "token")["data"][0].clone();
                let body = match page {
                    None => json!({"status":"ok", "statuscode":100, "data":[
                        {"id":"239", "xdg_type":"bin"}, {"id":244, "xdg_type":"bin"}, {"id":999, "xdg_type":"wallpaper"}
                    ]}),
                    Some(0) => json!({"status":"ok", "statuscode":100, "totalitems":20, "data":[
                        {"id":10,"name":"Directory link","downloadlink1":"https://example.test/downloads/","downloadname1":"downloads"}, first
                    ]}),
                    Some(_) => {
                        let mut second = first.clone();
                        second["id"] = json!(2259794);
                        second["name"] = json!("Painting tool");
                        second["summary"] = json!("<b>Paint</b> &amp; draw\n pictures");
                        json!({"status":"ok", "statuscode":100, "totalitems":20, "data":[first,second]})
                    }
                }.to_string();

                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });

        let manager = ProviderManager::new(None, None, None, Default::default()).unwrap();
        let results = manager
            .search_repositories(
                "paint & draw",
                &Provider::AppImageHub,
                Some(2),
                &RepositorySearchFilters::default(),
                Some(&format!("{base}/ocs/v1/")),
            )
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(
            results
                .iter()
                .map(|r| r.repo_slug.as_str())
                .collect::<Vec<_>>(),
            ["2259793", "2259794"]
        );

        assert_eq!(results[1].display_name, "Painting tool");
        assert_eq!(results[1].description, "Paint & draw pictures");
        assert_eq!(
            results[1].updated_at.to_rfc3339(),
            "2026-01-02T03:04:05+00:00"
        );

        assert_eq!(results[1].stars, 0);
        assert!(results[1].language.is_empty());
        assert_eq!(
            super::super::appimagehub_client::content_id(&results[1].repo_slug).unwrap(),
            "2259794"
        );
    }

    #[tokio::test]
    async fn manager_checks_versions_resolves_archived_tags_and_downloads_redirects() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server_base = base.clone();
        let server = thread::spawn(move || {
            let expected = [
                ("/ocs/v1/content/data/2259793?format=json", "v1.0.0"),
                ("/ocs/v1/content/data/2259793?format=json", "v1.1.0"),
                ("/ocs/v1/content/data/2259793?format=json", "v1.1.0"),
                (
                    "/ocs/v1/content/data/2259793/archived?format=json",
                    "v1.0.0",
                ),
                ("/download/archived", "redirect"),
                ("/files/Example.AppImage", "file"),
                ("/ocs/v1/content/data/2259793?format=json", "no-files"),
                ("/ocs/v1/content/data/2259793?format=json", "no-files"),
                (
                    "/ocs/v1/content/data/2259793/archived?format=json",
                    "v1.0.0",
                ),
                ("/ocs/v1/content/data/2259793?format=json", "v2.0.0-beta.1"),
            ];

            for (path, version) in expected {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line.split_whitespace().nth(1), Some(path));
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }

                if version == "redirect" {
                    write!(stream, "HTTP/1.1 302 Found\r\nLocation: {server_base}/files/Example.AppImage\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    let body = if version == "no-files" {
                        json!({"status":"ok", "statuscode":100, "data":[{"id":2259793}]})
                            .to_string()
                    } else if version == "file" {
                        "fixture artifact bytes".to_string()
                    } else {
                        response(
                            &server_base,
                            version,
                            if path.contains("archived") {
                                "archived"
                            } else {
                                version
                            },
                        )
                        .to_string()
                    };

                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            }
        });

        let manager = ProviderManager::new(None, None, None, Default::default()).unwrap();
        let api_root = format!("{base}/ocs/v1/");
        let mut package = Package::with_defaults(
            "appimagehub:2259793".into(),
            "2259793".into(),
            Filetype::AppImage,
            None,
            None,
            Channel::Stable,
            Provider::AppImageHub,
            Some(api_root.clone()),
        );

        let initial = manager
            .get_latest_release(
                &package.repo_slug,
                &package.provider,
                &package.channel,
                Some(&api_root),
            )
            .await
            .unwrap();

        package.record_release(&initial);
        assert_eq!(package.installed_release_tag().as_deref(), Some("v1.0.0"));
        let update = manager.check_for_updates(&package).await.unwrap().unwrap();
        assert_eq!(update.tag, "v1.1.0");
        assert!(package.is_update_available(&update));
        assert_ne!(
            initial.assets[0].download_url,
            update.assets[0].download_url
        );

        let archived = manager
            .get_release_by_tag(
                &package.repo_slug,
                "v1.0.0",
                &package.provider,
                Some(&api_root),
            )
            .await
            .unwrap();

        let root = test_support::temp_root("appimagehub", "download");
        let path = manager
            .download_asset(
                &archived.assets[0],
                &package.provider,
                &root,
                &mut None::<fn(u64, u64)>,
            )
            .await
            .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"fixture artifact bytes");
        let mut database =
            crate::storage::database::PackageDatabase::open(&root.join("packages.db")).unwrap();

        database.upsert_package(&package).unwrap();
        let restored = database.get_package(&package.id).unwrap().unwrap();
        assert_eq!(restored.provider, Provider::AppImageHub);
        assert_eq!(restored.installed_release_tag().as_deref(), Some("v1.0.0"));
        let reference = crate::models::upstream::PackageReference::from_package(restored);
        assert_eq!(reference.version_tag.as_deref(), Some("v1.0.0"));
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            manager
                .get_latest_release(
                    "2259793",
                    &package.provider,
                    &Channel::Stable,
                    Some(&api_root)
                )
                .await
                .unwrap_err()
                .to_string()
                .contains("directory-only")
        );

        assert_eq!(
            manager
                .get_release_by_tag("2259793", "v1.0.0", &package.provider, Some(&api_root))
                .await
                .unwrap()
                .tag,
            "v1.0.0"
        );

        assert_eq!(
            manager
                .get_latest_release(
                    "2259793",
                    &package.provider,
                    &Channel::Stable,
                    Some(&api_root)
                )
                .await
                .unwrap()
                .tag,
            "v0.9.0"
        );

        server.join().unwrap();
        assert!(
            manager
                .search_repositories(
                    "test",
                    &Provider::AppImageHub,
                    None,
                    &RepositorySearchFilters {
                        language: Some("Rust".into()),
                        ..Default::default()
                    },
                    None
                )
                .await
                .unwrap_err()
                .to_string()
                .contains("not supported")
        );

        assert!(
            manager
                .get_branch_head_sha("2259793", &Provider::AppImageHub, "main", None)
                .await
                .is_err()
        );
    }
}
