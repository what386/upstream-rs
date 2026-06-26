use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::path::Path;

use crate::models::common::{Version, enums::Filetype};
use crate::models::provider::{Asset, Release};
use crate::providers::http::http_client::{ConditionalDiscoveryResult, HttpAssetInfo, HttpClient};
use crate::providers::release_provider::ReleaseProvider;
use crate::utils::filename_parser::parse_filetype;

#[derive(Debug, Clone)]
pub struct WebScraperAdapter {
    client: HttpClient,
}

impl WebScraperAdapter {
    fn parse_version_from_filename(filename: &str) -> Option<Version> {
        Version::from_filename(filename).ok()
    }

    fn version_from_last_modified(dt: DateTime<Utc>) -> Version {
        // Monotonic semver-like mapping for stable update comparisons.
        let major = dt.year_ce().1;
        let minor = dt.ordinal();
        let patch = dt.num_seconds_from_midnight();
        Version::new(major, minor, patch, false)
    }

    fn is_unversioned_download_asset(info: &HttpAssetInfo) -> bool {
        if Self::parse_version_from_filename(&info.name).is_some() {
            return false;
        }

        matches!(
            parse_filetype(&info.name),
            Filetype::AppImage
                | Filetype::MacApp
                | Filetype::MacDmg
                | Filetype::Archive
                | Filetype::Compressed
                | Filetype::WinExe
        )
    }

    fn select_infos_for_best_version(
        infos: &[HttpAssetInfo],
        best_version: Option<&Version>,
    ) -> Vec<HttpAssetInfo> {
        let Some(target_version) = best_version else {
            return infos.to_vec();
        };

        let filtered: Vec<_> = infos
            .iter()
            .filter(|info| {
                Self::parse_version_from_filename(&info.name)
                    .map(|v| v.cmp(target_version).is_eq())
                    .unwrap_or_else(|| Self::is_unversioned_download_asset(info))
            })
            .cloned()
            .collect();

        if filtered.is_empty() {
            infos.to_vec()
        } else {
            filtered
        }
    }

    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }

    pub async fn download_asset<F>(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: &mut Option<F>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
    {
        self.client
            .download_file(&asset.download_url, destination_path, dl_callback)
            .await
    }

    pub async fn get_release_by_tag(&self, _slug: &str, _tag: &str) -> Result<Release> {
        bail!("HTTP provider does not support tagged releases")
    }

    pub async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        self.get_latest_release_if_modified_since(slug, None)
            .await?
            .ok_or_else(|| anyhow!("Unexpected not-modified response for scraper provider"))
    }

    pub async fn get_latest_release_if_modified_since(
        &self,
        slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        let discovery = self
            .client
            .discover_assets_if_modified_since(slug, last_upgraded)
            .await?;
        let mut infos = match discovery {
            ConditionalDiscoveryResult::NotModified => return Ok(None),
            ConditionalDiscoveryResult::Assets(infos) => infos,
        };

        let mut best_version: Option<Version> = None;
        for info in &infos {
            if let Some(version) = Self::parse_version_from_filename(&info.name) {
                match &best_version {
                    Some(prev) if prev.cmp(&version).is_ge() => {}
                    _ => best_version = Some(version),
                }
            }
        }

        if best_version.is_none() {
            let hydrate_limit = infos.len().min(24);
            for info in infos.iter_mut().take(hydrate_limit) {
                let url = info.download_url.clone();
                if let Ok(probed) = self.client.probe_asset(&url).await {
                    info.size = probed.size;
                    if probed.last_modified.is_some() {
                        info.last_modified = probed.last_modified;
                    }
                    if probed.etag.is_some() {
                        info.etag = probed.etag;
                    }
                }
            }
        }

        if best_version.is_none() {
            for info in &infos {
                if let Some(last_modified) = info.last_modified {
                    let version = Self::version_from_last_modified(last_modified);
                    match &best_version {
                        Some(prev) if prev.cmp(&version).is_ge() => {}
                        _ => best_version = Some(version),
                    }
                }
            }
        }

        let selected_infos = Self::select_infos_for_best_version(&infos, best_version.as_ref());

        let published_at = selected_infos
            .iter()
            .filter_map(|i| i.last_modified)
            .max()
            .unwrap_or_else(|| last_upgraded.unwrap_or_else(Utc::now));

        let assets: Vec<Asset> = selected_infos
            .iter()
            .enumerate()
            .map(|(idx, info)| {
                Asset::new(
                    info.download_url.clone(),
                    (idx + 1) as u64,
                    info.name.clone(),
                    info.size,
                    info.last_modified.unwrap_or(published_at),
                )
            })
            .collect();

        let version = best_version.unwrap_or_else(|| Version::new(0, 0, 0, false));
        let release_name = if assets.len() == 1 {
            let info = &selected_infos[0];
            if let Some(etag) = &info.etag {
                format!("{} [{}]", info.name, etag)
            } else {
                info.name.clone()
            }
        } else {
            format!("Discovered {} assets", assets.len())
        };
        Ok(Some(Release {
            id: 1,
            tag: "direct".to_string(),
            name: release_name,
            body: "Discovered from HTTP source".to_string(),
            is_draft: false,
            is_prerelease: false,
            assets,
            version,
            published_at,
        }))
    }

    pub async fn get_releases(
        &self,
        slug: &str,
        _per_page: Option<u32>,
        _max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        Ok(vec![self.get_latest_release(slug).await?])
    }
}

#[async_trait::async_trait(?Send)]
impl ReleaseProvider for WebScraperAdapter {
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        WebScraperAdapter::get_latest_release(self, slug).await
    }

    async fn get_releases(
        &self,
        slug: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<Release>> {
        WebScraperAdapter::get_releases(self, slug, per_page, max_total).await
    }

    async fn get_release_by_tag(&self, slug: &str, tag: &str) -> Result<Release> {
        WebScraperAdapter::get_release_by_tag(self, slug, tag).await
    }

    async fn get_latest_release_if_modified_since(
        &self,
        slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        WebScraperAdapter::get_latest_release_if_modified_since(self, slug, last_upgraded).await
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination_path: &Path,
        dl_callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        let mut forwarded = dl_callback;
        WebScraperAdapter::download_asset(self, asset, destination_path, &mut forwarded).await
    }
}

#[cfg(test)]
mod tests {
    use super::{HttpAssetInfo, WebScraperAdapter};
    use crate::models::common::Version;
    use crate::providers::http::HttpClient;
    use chrono::Utc;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    fn spawn_test_server<F>(max_requests: usize, handler: F) -> String
    where
        F: Fn(&str, &str) -> String + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let addr = listener.local_addr().expect("resolve local addr");
            tx.send(addr).expect("send test server addr");

            for _ in 0..max_requests {
                let (mut stream, _) = listener.accept().expect("accept request");
                let cloned = stream.try_clone().expect("clone stream");
                let mut reader = BufReader::new(cloned);

                let mut request_line = String::new();
                reader
                    .read_line(&mut request_line)
                    .expect("read request line");
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("/");

                let mut line = String::new();
                loop {
                    line.clear();
                    reader.read_line(&mut line).expect("read request headers");
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }

                let response = handler(method, path);
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
                stream.flush().expect("flush response");
            }
        });

        let addr = rx.recv().expect("receive server address");
        format!("http://{}", addr)
    }

    fn http_response(status_line: &str, headers: &[(&str, &str)], body: &str) -> String {
        let mut out = format!("{status_line}\r\n");
        for (k, v) in headers {
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str("\r\n");
        out.push_str(body);
        out
    }

    fn fixture_response(body: &'static str) -> String {
        http_response(
            "HTTP/1.1 200 OK",
            &[
                ("Connection", "close"),
                ("Content-Type", "text/html"),
                ("Content-Length", &body.len().to_string()),
            ],
            body,
        )
    }

    fn asset_names(release: &crate::models::provider::Release) -> Vec<&str> {
        release
            .assets
            .iter()
            .map(|asset| asset.name.as_str())
            .collect()
    }

    #[test]
    fn parse_version_from_filename_extracts_semver_triplet() {
        let version = WebScraperAdapter::parse_version_from_filename("tool-v1.4.9-linux.tar.gz")
            .expect("parsed version");
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 4);
        assert_eq!(version.patch, 9);
    }

    fn test_asset(name: &str) -> HttpAssetInfo {
        HttpAssetInfo {
            download_url: format!("https://example.invalid/{name}"),
            name: name.to_string(),
            size: 0,
            last_modified: None,
            etag: None,
        }
    }

    #[test]
    fn version_filter_keeps_unversioned_download_assets() {
        let infos = vec![
            test_asset("ffmpeg-release-essentials.7z"),
            test_asset("ffmpeg-release-essentials.zip"),
            test_asset("ffmpeg-release-github"),
            test_asset("ffmpeg-release-essentials.7z.ver"),
            test_asset("ffmpeg-8.0.1-essentials_build.7z"),
            test_asset("ffmpeg-8.0.1-full_build.7z"),
            test_asset("ffmpeg-7.1.1-full_build.7z"),
        ];

        let selected = WebScraperAdapter::select_infos_for_best_version(
            &infos,
            Some(&Version::new(8, 0, 1, false)),
        );
        let names: Vec<_> = selected.iter().map(|info| info.name.as_str()).collect();

        assert!(names.contains(&"ffmpeg-release-essentials.7z"));
        assert!(names.contains(&"ffmpeg-release-essentials.zip"));
        assert!(names.contains(&"ffmpeg-8.0.1-essentials_build.7z"));
        assert!(names.contains(&"ffmpeg-8.0.1-full_build.7z"));
        assert!(!names.contains(&"ffmpeg-release-github"));
        assert!(!names.contains(&"ffmpeg-release-essentials.7z.ver"));
        assert!(!names.contains(&"ffmpeg-7.1.1-full_build.7z"));
    }

    #[tokio::test]
    async fn get_latest_release_selects_assets_for_latest_detected_version() {
        let html = include_str!(
            "../../../tests/fixtures/providers/http/snippets/latest-version-links.html"
        )
        .to_string();
        let html_len = html.len().to_string();
        let html_for_server = html.clone();
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "GET");
            http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Connection", "close"),
                    ("Content-Type", "text/html"),
                    ("Content-Length", &html_len),
                ],
                &html_for_server,
            )
        });

        let adapter =
            WebScraperAdapter::new(HttpClient::new(Default::default()).expect("http client"));
        let release = adapter
            .get_latest_release(&server)
            .await
            .expect("latest release");

        assert_eq!(release.version.major, 1);
        assert_eq!(release.version.minor, 10);
        assert_eq!(release.version.patch, 0);
        assert_eq!(release.assets.len(), 1);
        assert!(release.assets[0].name.contains("1.10.0"));
    }

    #[tokio::test]
    async fn fixture_ffmpeg_builds_page_keeps_latest_release_downloads() {
        let html = include_str!("../../../tests/fixtures/providers/http/ffmpeg.html");
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "GET");
            fixture_response(html)
        });

        let adapter =
            WebScraperAdapter::new(HttpClient::new(Default::default()).expect("http client"));
        let release = adapter
            .get_latest_release(&server)
            .await
            .expect("latest release");
        let names = asset_names(&release);

        assert_eq!(release.version, Version::new(8, 0, 1, false));
        assert!(names.contains(&"ffmpeg-release-essentials.7z"));
        assert!(names.contains(&"ffmpeg-release-essentials.zip"));
        assert!(names.contains(&"ffmpeg-release-full.7z"));
        assert!(names.contains(&"ffmpeg-release-full-shared.7z"));
        assert!(names.contains(&"ffmpeg-8.0.1-essentials_build.7z"));
        assert!(names.contains(&"ffmpeg-8.0.1-full_build.7z"));
        assert!(names.iter().all(|name| !name.ends_with(".sha256")));
        assert!(names.iter().all(|name| !name.ends_with(".ver")));
        assert!(!names.contains(&"ffmpeg-release-github"));
    }

    #[tokio::test]
    async fn fixture_zig_builds_page_selects_current_build_assets() {
        let html = include_str!("../../../tests/fixtures/providers/http/zig.html");
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "GET");
            fixture_response(html)
        });

        let adapter =
            WebScraperAdapter::new(HttpClient::new(Default::default()).expect("http client"));
        let release = adapter
            .get_latest_release(&server)
            .await
            .expect("latest release");
        let names = asset_names(&release);

        assert_eq!(release.version, Version::new(0, 17, 0, false));
        assert!(names.contains(&"zig-0.17.0-dev.813+2153f8143.tar.xz"));
        assert!(names.contains(&"zig-bootstrap-0.17.0-dev.813+2153f8143.tar.xz"));
        assert!(names.contains(&"zig-x86_64-linux-0.17.0-dev.813+2153f8143.tar.xz"));
        assert!(names.contains(&"zig-x86_64-windows-0.17.0-dev.813+2153f8143.zip"));
        assert!(names.iter().all(|name| !name.ends_with(".minisig")));
    }

    #[tokio::test]
    async fn get_latest_release_uses_html_last_modified_for_unversioned_links() {
        let html =
            include_str!("../../../tests/fixtures/providers/http/snippets/unversioned-link.html")
                .to_string();
        let html_len = html.len().to_string();
        let html_for_server = html.clone();
        let server = spawn_test_server(2, move |method, path| match (method, path) {
            ("GET", "/") => http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Connection", "close"),
                    ("Content-Type", "text/html"),
                    ("Last-Modified", "Tue, 10 Feb 2026 15:04:05 GMT"),
                    ("Content-Length", &html_len),
                ],
                &html_for_server,
            ),
            ("HEAD", "/tool-release.zip") => http_response(
                "HTTP/1.1 200 OK",
                &[("Connection", "close"), ("Content-Length", "0")],
                "",
            ),
            _ => panic!("unexpected request {method} {path}"),
        });

        let adapter =
            WebScraperAdapter::new(HttpClient::new(Default::default()).expect("http client"));
        let release = adapter
            .get_latest_release(&server)
            .await
            .expect("latest release");

        assert_eq!(release.version, Version::new(2026, 41, 54245, false));
        assert_eq!(release.published_at, release.assets[0].created_at);
    }

    #[tokio::test]
    async fn conditional_latest_release_returns_none_on_not_modified() {
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "GET");
            http_response("HTTP/1.1 304 Not Modified", &[("Connection", "close")], "")
        });
        let adapter =
            WebScraperAdapter::new(HttpClient::new(Default::default()).expect("http client"));
        let release = adapter
            .get_latest_release_if_modified_since(&server, Some(Utc::now()))
            .await
            .expect("conditional release");
        assert!(release.is_none());
    }
}
