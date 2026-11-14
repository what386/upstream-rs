use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use futures_util::{StreamExt, stream};
use scraper::{Html, Selector};
use std::{collections::HashSet, path::Path, time::Duration};

use crate::{
    models::{
        common::{Version, enums::Filetype},
        provider::{Asset, Release},
    },
    providers::assets::filenames::parser::{parse_arch, parse_filetype, parse_os},
    providers::sites::{
        ReleaseProvider,
        http::{ConditionalDocumentResult, HttpAssetInfo, HttpClient},
    },
    utils::platform::platform_info::{ArchitectureInfo, CpuArch},
};

const MAX_CANDIDATE_PROBES: usize = 24;
const PROBE_CONCURRENCY: usize = 4;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct WebScraperAdapter {
    client: HttpClient,
}

#[derive(Debug, Clone)]
struct Candidate {
    url: String,
    score: i32,
    version: Option<Version>,
}

impl WebScraperAdapter {
    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }

    fn version_from_last_modified(dt: DateTime<Utc>) -> Version {
        Version::new(
            dt.year_ce().1,
            dt.ordinal(),
            dt.num_seconds_from_midnight(),
            false,
        )
    }

    fn highest_version(versions: impl IntoIterator<Item = Version>) -> Option<Version> {
        versions
            .into_iter()
            .fold(None, |best, candidate| match best {
                Some(current)
                    if !matches!(
                        candidate.partial_cmp(&current),
                        Some(std::cmp::Ordering::Greater)
                    ) =>
                {
                    Some(current)
                }
                _ => Some(candidate),
            })
    }

    fn is_download_filetype(filetype: Filetype) -> bool {
        matches!(
            filetype,
            Filetype::AppImage | Filetype::Archive | Filetype::Compressed | Filetype::WinExe
        )
    }

    fn candidate_score(name: &str, text: &str, explicit_download: bool) -> i32 {
        let filetype = parse_filetype(name);
        let mut score =
            if Path::new(name).extension().is_some() && Self::is_download_filetype(filetype) {
                100
            } else {
                0
            };

        let text = text.to_ascii_lowercase();
        if name.to_ascii_lowercase().ends_with(".html") {
            return -200;
        }

        if explicit_download {
            score += 80;
        }

        if ["download", "get it", "install"]
            .iter()
            .any(|word| text.contains(word))
        {
            score += 40;
        }

        if ["checksum", "sha256", "signature", "minisig", "asc"]
            .iter()
            .any(|word| text.contains(word))
        {
            score -= 200;
        }

        score
    }

    fn extract_candidates(base: &reqwest::Url, html: &str) -> Vec<Candidate> {
        let document = Html::parse_document(html);
        let selector = Selector::parse("a[href], [data-download-url], [data-download], [data-url]")
            .expect("valid static selector");

        let mut seen = HashSet::new();
        let mut candidates = Vec::new();

        for element in document.select(&selector) {
            let explicit_download = element.value().attr("download").is_some()
                || element.value().attr("data-download-url").is_some()
                || element.value().attr("data-download").is_some();

            let value = ["data-download-url", "data-download", "href", "data-url"]
                .iter()
                .find_map(|attribute| element.value().attr(attribute));

            let Some(value) = value else {
                continue;
            };

            if value.starts_with('#')
                || value.starts_with("javascript:")
                || value.starts_with("mailto:")
                || value.starts_with("tel:")
            {
                continue;
            }

            let Ok(url) = base.join(value) else {
                continue;
            };

            if !matches!(url.scheme(), "http" | "https") {
                continue;
            }

            let url = url.to_string();
            let name = HttpClient::file_name_from_url(&url);
            if parse_filetype(&name) == Filetype::Checksum {
                continue;
            }

            let text = element.text().collect::<String>();
            let score = Self::candidate_score(&name, &text, explicit_download);
            if score <= 0 || !seen.insert(url.clone()) {
                continue;
            }

            candidates.push(Candidate {
                version: Version::from_filename(&name).ok(),
                url,
                score,
            });
        }

        candidates.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.url.cmp(&right.url))
        });
        candidates
    }

    async fn validate_candidates(
        &self,
        candidates: Vec<Candidate>,
        page_modified: Option<DateTime<Utc>>,
        page_etag: Option<String>,
        probe_timeout: Duration,
    ) -> Vec<(Candidate, HttpAssetInfo)> {
        let mut validated = stream::iter(candidates.into_iter().enumerate())
            .map(|(index, candidate)| {
                let page_etag = page_etag.clone();
                async move {
                    // One deadline covers HEAD, redirects and any GET fallback.
                    let info = tokio::time::timeout(
                        probe_timeout,
                        self.client.probe_asset(&candidate.url),
                    )
                    .await
                    .ok()?
                    .ok()?;

                    if info.content_type.as_deref().is_some_and(|kind| {
                        kind.contains("text/html") || kind.contains("application/xhtml")
                    }) || parse_filetype(&info.name) == Filetype::Checksum
                    {
                        return None;
                    }

                    let mut info = info;
                    if info.last_modified.is_none() {
                        info.last_modified = page_modified;
                    }

                    if info.etag.is_none() {
                        info.etag = page_etag;
                    }

                    Some((index, candidate, info))
                }
            })
            .buffer_unordered(PROBE_CONCURRENCY)
            .filter_map(|result| async move { result })
            .collect::<Vec<_>>()
            .await;

        // Network completion order must not change asset ordering or IDs.
        validated.sort_by_key(|(index, _, _)| *index);
        validated
            .into_iter()
            .map(|(_, candidate, info)| (candidate, info))
            .collect()
    }

    fn candidate_platform_priority(candidate: &Candidate, platform: &ArchitectureInfo) -> u8 {
        let name = HttpClient::file_name_from_url(&candidate.url);
        let os = parse_os(&name);
        let arch = parse_arch(&name);
        if os.as_ref().is_some_and(|os| os != &platform.os_kind) {
            return 0;
        }

        let arch_score = match arch {
            Some(ref arch) if arch == &platform.cpu_arch => 3,
            Some(CpuArch::X86) if platform.cpu_arch == CpuArch::X86_64 => 1,
            Some(CpuArch::Arm) if platform.cpu_arch == CpuArch::Aarch64 => 1,
            None => 2,
            Some(_) => return 0,
        };

        arch_score + if os.is_some() { 4 } else { 0 }
    }

    fn select_candidates(
        candidates: Vec<Candidate>,
        platform: &ArchitectureInfo,
    ) -> Vec<Candidate> {
        // Rank for this host before applying the request budget. Unknown targets
        // remain eligible for generic download URLs; incompatible targets can
        // still fill spare slots for probe's cross-platform listing.
        let mut ranked = candidates
            .into_iter()
            .map(|candidate| {
                let priority = Self::candidate_platform_priority(&candidate, platform);
                let version_kind = match candidate.version.as_ref() {
                    Some(Version::Semver { .. }) => 2,
                    Some(Version::Datetime { .. }) => 1,
                    _ => 0,
                };

                (priority, version_kind, candidate)
            })
            .collect::<Vec<_>>();

        // Within otherwise equal candidates, newer versions should not lose
        // their budget slots to lexicographically earlier historical releases.
        ranked.sort_by(
            |(left_priority, left_kind, left), (right_priority, right_kind, right)| {
                right_priority
                    .cmp(left_priority)
                    .then_with(|| right.score.cmp(&left.score))
                    .then_with(|| right_kind.cmp(left_kind))
                    .then_with(|| match (&left.version, &right.version) {
                        (Some(left), Some(right)) => {
                            right.partial_cmp(left).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        _ => std::cmp::Ordering::Equal,
                    })
                    .then_with(|| left.url.cmp(&right.url))
            },
        );
        ranked
            .into_iter()
            .take(MAX_CANDIDATE_PROBES)
            .map(|(_, _, candidate)| candidate)
            .collect()
    }

    fn select_latest(infos: Vec<(Candidate, HttpAssetInfo)>) -> Vec<HttpAssetInfo> {
        let best = Self::highest_version(
            infos
                .iter()
                .filter_map(|(candidate, _)| candidate.version.clone()),
        );

        let selected: Vec<_> = infos
            .into_iter()
            .filter(|(candidate, _)| {
                best.as_ref().is_none_or(|version| {
                    candidate
                        .version
                        .as_ref()
                        .is_none_or(|candidate_version| candidate_version == version)
                })
            })
            .map(|(_, info)| info)
            .collect();

        selected
    }

    async fn release_since(
        &self,
        slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        let document = tokio::time::timeout(
            PAGE_TIMEOUT,
            self.client
                .fetch_document_if_modified_since(slug, last_upgraded),
        )
        .await
        .with_context(|| format!("Timed out fetching download page '{slug}'"))??;

        let ConditionalDocumentResult::Document(document) = document else {
            return Ok(None);
        };

        if !document.content_type.contains("text/html")
            && !document.content_type.contains("application/xhtml")
        {
            let info = HttpClient::asset_info(&document.url, &document.headers);
            return Ok(Some(Self::release_from_infos(vec![info], last_upgraded)));
        }

        let base = reqwest::Url::parse(&document.url)
            .context("Failed to parse final download page URL")?;

        let html = String::from_utf8_lossy(&document.body);
        let candidates = Self::select_candidates(
            Self::extract_candidates(&base, &html),
            &ArchitectureInfo::new(),
        );

        if candidates.is_empty() {
            bail!("No download links found on '{}'", document.url);
        }

        let page_modified = document
            .headers
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
            .map(|value| value.with_timezone(&Utc));

        let page_etag = document
            .headers
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim_matches('"').to_string());

        let infos = Self::select_latest(
            self.validate_candidates(candidates, page_modified, page_etag, PROBE_TIMEOUT)
                .await,
        );

        if infos.is_empty() {
            bail!("No downloadable assets found on '{}'", document.url);
        }

        Ok(Some(Self::release_from_infos(infos, last_upgraded)))
    }

    fn release_from_infos(
        infos: Vec<HttpAssetInfo>,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Release {
        let published_at = infos
            .iter()
            .filter_map(|info| info.last_modified)
            .max()
            .unwrap_or_else(|| last_upgraded.unwrap_or_else(Utc::now));

        let version = Self::highest_version(
            infos
                .iter()
                .filter_map(|info| Version::from_filename(&info.name).ok()),
        )
        .unwrap_or_else(|| Self::version_from_last_modified(published_at));

        let name = if infos.len() == 1 {
            infos[0].name.clone()
        } else {
            format!("Discovered {} assets", infos.len())
        };

        Release {
            id: 1,
            tag: "scraped".to_string(),
            name,
            body: "Discovered from download page".to_string(),
            is_draft: false,
            is_prerelease: false,
            assets: infos
                .into_iter()
                .enumerate()
                .map(|(index, info)| {
                    Asset::new(
                        info.download_url,
                        (index + 1) as u64,
                        info.name,
                        info.size,
                        info.last_modified.unwrap_or(published_at),
                    )
                })
                .collect(),
            version,
            published_at,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl ReleaseProvider for WebScraperAdapter {
    async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        self.release_since(slug, None)
            .await?
            .ok_or_else(|| anyhow!("Unexpected not-modified response for scraper provider"))
    }

    async fn get_releases(
        &self,
        slug: &str,
        _: Option<u32>,
        _: Option<u32>,
    ) -> Result<Vec<Release>> {
        Ok(vec![self.get_latest_release(slug).await?])
    }

    async fn get_release_by_tag(&self, _: &str, _: &str) -> Result<Release> {
        bail!("Scraper provider does not support tagged releases")
    }

    async fn get_latest_release_since(
        &self,
        slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        self.release_since(slug, last_upgraded).await
    }

    async fn download_asset(
        &self,
        asset: &Asset,
        destination: &Path,
        callback: Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        let mut callback = callback;
        self.client
            .download_file(&asset.download_url, destination, &mut callback)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::{Candidate, MAX_CANDIDATE_PROBES, PROBE_CONCURRENCY, WebScraperAdapter};
    use crate::{
        models::{common::Version, upstream::config::DownloadConfig},
        providers::sites::{ReleaseProvider, http::HttpClient},
        utils::platform::platform_info::{ArchitectureInfo, CpuArch, OSKind},
    };
    use chrono::Utc;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    struct TestServer {
        url: String,
        requests: Arc<Mutex<Vec<(String, String)>>>,
        peak: Arc<AtomicUsize>,
        stop: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn new(handler: impl Fn(&str, &str) -> String + Send + Sync + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let peak = Arc::new(AtomicUsize::new(0));
            let stop = Arc::new(AtomicBool::new(false));
            let worker = {
                let requests = Arc::clone(&requests);
                let peak = Arc::clone(&peak);
                let stop = Arc::clone(&stop);
                let handler = Arc::new(handler);
                let active = Arc::new(AtomicUsize::new(0));
                thread::spawn(move || {
                    let mut connections = Vec::new();
                    while !stop.load(Ordering::SeqCst) {
                        let (mut socket, _) = match listener.accept() {
                            Ok(connection) => connection,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(1));
                                continue;
                            }
                            Err(error) => panic!("accept fixture request: {error}"),
                        };

                        let requests = Arc::clone(&requests);
                        let peak = Arc::clone(&peak);
                        let active = Arc::clone(&active);
                        let handler = Arc::clone(&handler);
                        connections.push(thread::spawn(move || {
                            // Accepted sockets can inherit the listener's nonblocking
                            // mode on Windows. Request parsing below uses blocking I/O.
                            socket.set_nonblocking(false).unwrap();
                            socket
                                .set_read_timeout(Some(Duration::from_secs(1)))
                                .unwrap();
                            let mut reader = BufReader::new(socket.try_clone().unwrap());
                            let mut line = String::new();
                            reader.read_line(&mut line).unwrap();
                            let mut parts = line.split_whitespace();
                            let method = parts.next().unwrap_or("").to_string();
                            let path = parts.next().unwrap_or("").to_string();
                            loop {
                                line.clear();
                                reader.read_line(&mut line).unwrap();
                                if line.is_empty() || line == "\r\n" {
                                    break;
                                }
                            }

                            requests
                                .lock()
                                .unwrap()
                                .push((method.clone(), path.clone()));
                            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(count, Ordering::SeqCst);
                            let response = handler(&method, &path);
                            active.fetch_sub(1, Ordering::SeqCst);
                            // Timeout tests deliberately close sockets before the response.
                            let _ = socket.write_all(response.as_bytes());
                        }));
                    }

                    for connection in connections {
                        connection.join().unwrap();
                    }
                })
            };

            Self {
                url,
                requests,
                peak,
                stop,
                worker: Some(worker),
            }
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    fn response(status: &str, kind: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn candidate(url: String) -> Candidate {
        Candidate {
            version: Version::from_filename(&HttpClient::file_name_from_url(&url)).ok(),
            url,
            score: 100,
        }
    }

    fn adapter() -> WebScraperAdapter {
        WebScraperAdapter::new(HttpClient::new(DownloadConfig::default()).unwrap())
    }

    #[test]
    fn host_downloads_and_newer_versions_are_ranked_before_the_probe_cap() {
        let platform = ArchitectureInfo {
            is_os_64_bit: true,
            cpu_arch: CpuArch::X86_64,
            os_kind: OSKind::Linux,
        };

        let mut candidates = (0..40)
            .map(|index| {
                candidate(format!(
                    "https://example.invalid/a-tool-v1.0.0-macos-aarch64-{index}.tar.gz"
                ))
            })
            .collect::<Vec<_>>();

        let newest = candidate("https://example.invalid/z-tool-v2.0.0-linux-x86_64.tar.gz".into());
        let generic = candidate("https://example.invalid/download.tar.gz".into());
        candidates.push(candidate(
            "https://example.invalid/z-tool-v1.0.0-linux-x86_64.tar.gz".into(),
        ));
        candidates.push(newest.clone());
        candidates.push(generic.clone());
        let selected = WebScraperAdapter::select_candidates(candidates, &platform);
        assert_eq!(selected.len(), MAX_CANDIDATE_PROBES);
        assert_eq!(selected[0].url, newest.url);
        assert!(selected.iter().any(|item| item.url == generic.url));

        let mut history = (1..40)
            .map(|minor| {
                candidate(format!(
                    "https://example.invalid/tool-v1.{minor}.0-linux-x86_64.tar.gz"
                ))
            })
            .collect::<Vec<_>>();

        history.push(newest.clone());
        let selected = WebScraperAdapter::select_candidates(history, &platform);
        assert_eq!(selected[0].url, newest.url);
        assert_eq!(selected.len(), MAX_CANDIDATE_PROBES);
    }

    #[tokio::test]
    async fn probes_run_with_bounded_concurrency_and_preserve_order_and_page_metadata() {
        let server = TestServer::new(|method, path| {
            assert_eq!(method, "HEAD");
            thread::sleep(if path.starts_with("/0-") {
                Duration::from_millis(150)
            } else {
                Duration::from_millis(30)
            });
            response("200 OK", "application/octet-stream", "")
        });

        let candidates = (0..9)
            .map(|index| candidate(format!("{}/{index}-tool-v1.0.0.tar.gz", server.url)))
            .collect::<Vec<_>>();

        let expected = candidates
            .iter()
            .map(|item| item.url.clone())
            .collect::<Vec<_>>();

        let modified = Utc::now();
        let validated = adapter()
            .validate_candidates(
                candidates,
                Some(modified),
                Some("page-etag".into()),
                Duration::from_secs(2),
            )
            .await;

        assert_eq!(
            validated
                .iter()
                .map(|(item, _)| item.url.clone())
                .collect::<Vec<_>>(),
            expected
        );

        assert!(
            validated
                .iter()
                .all(|(_, info)| info.last_modified == Some(modified)
                    && info.etag.as_deref() == Some("page-etag"))
        );

        let peak = server.peak.load(Ordering::SeqCst);
        assert!(
            peak > 1 && peak <= PROBE_CONCURRENCY,
            "unexpected concurrency: {peak}"
        );
    }

    #[tokio::test]
    async fn stalled_heads_and_get_fallbacks_time_out_without_losing_fast_assets() {
        let server = TestServer::new(|method, path| {
            if path.starts_with("/slow-head") || (path.starts_with("/fallback") && method == "GET")
            {
                thread::sleep(Duration::from_millis(400));
            }

            if path.starts_with("/fallback") && method == "HEAD" {
                response("405 Method Not Allowed", "text/plain", "")
            } else {
                response("200 OK", "application/octet-stream", "")
            }
        });

        let candidates = ["slow-head", "fallback", "fast"]
            .map(|name| candidate(format!("{}/{name}-v1.0.0.tar.gz", server.url)))
            .to_vec();

        let validated = adapter()
            .validate_candidates(candidates, None, None, Duration::from_millis(100))
            .await;

        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].1.name, "fast-v1.0.0.tar.gz");
        let requests = server.requests.lock().unwrap();
        assert!(
            requests
                .iter()
                .any(|(method, path)| method == "GET" && path.starts_with("/fallback"))
        );

        assert!(
            !requests
                .iter()
                .any(|(method, path)| method == "GET" && path.starts_with("/slow-head"))
        );
    }

    #[tokio::test]
    async fn release_discovery_validates_assets_without_crawling_html_links() {
        let server = TestServer::new(|method, path| {
            if path == "/downloads/" {
                assert_eq!(method, "GET");
                response(
                    "200 OK",
                    "text/html",
                    r#"<a href="/tool-v1.2.3.tar.gz">Download</a><a href="/download-docs">Download docs</a><a href="/tool-v1.2.3.tar.gz.sha256">Checksum</a>"#,
                )
            } else if path == "/download-docs" {
                assert_eq!(method, "HEAD");
                response("200 OK", "text/html", "")
            } else {
                assert_eq!(path, "/tool-v1.2.3.tar.gz");
                assert_eq!(method, "HEAD");
                response("200 OK", "application/octet-stream", "")
            }
        });

        let release = adapter()
            .get_latest_release(&format!("{}/downloads/", server.url))
            .await
            .unwrap();

        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "tool-v1.2.3.tar.gz");
        assert_eq!(release.version, Version::new(1, 2, 3, false));
        assert_eq!(server.requests.lock().unwrap().len(), 3);
    }

    #[test]
    fn structured_extraction_keeps_download_controls_and_ignores_page_noise() {
        let page = include_str!("../../../../test-server/pages/downloads/standard.html");
        let base = reqwest::Url::parse("https://example.invalid/pages/downloads/standard.html")
            .expect("valid base URL");

        let candidates = WebScraperAdapter::extract_candidates(&base, page);
        let urls: Vec<_> = candidates
            .iter()
            .map(|candidate| candidate.url.as_str())
            .collect();

        assert_eq!(urls.len(), 6);
        assert!(urls.iter().any(|url| url.ends_with("linux-x86_64.tar.gz")));
        assert!(urls.iter().any(|url| url.ends_with("linux-x86_64.zip")));
        assert!(urls.iter().all(|url| !url.ends_with(".sha256")));
        assert!(urls.iter().all(|url| !url.ends_with("standard.html")));
    }

    #[test]
    fn latest_version_selection_excludes_old_release_assets() {
        let page = include_str!("../../../../test-server/pages/downloads/release-history.html");
        let base =
            reqwest::Url::parse("https://example.invalid/pages/downloads/release-history.html")
                .expect("valid base URL");

        let candidates = WebScraperAdapter::extract_candidates(&base, page);
        let versions: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| candidate.version.clone())
            .collect();

        assert_eq!(
            WebScraperAdapter::highest_version(versions),
            Some(Version::new(1, 0, 0, false))
        );
    }
}
