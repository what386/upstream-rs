use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::http::http_client::{ConditionalDiscoveryResult, HttpClient};

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
            for idx in 0..hydrate_limit {
                let url = infos[idx].download_url.clone();
                if let Ok(probed) = self.client.probe_asset(&url).await {
                    infos[idx].size = probed.size;
                    infos[idx].last_modified = probed.last_modified;
                    infos[idx].etag = probed.etag;
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

        let selected_infos = if let Some(target_version) = &best_version {
            let filtered: Vec<_> = infos
                .iter()
                .filter(|info| {
                    Self::parse_version_from_filename(&info.name)
                        .map(|v| v.cmp(target_version).is_eq())
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            if filtered.is_empty() { infos } else { filtered }
        } else {
            infos
        };

        let published_at = selected_infos
            .iter()
            .filter_map(|i| i.last_modified)
            .max()
            .unwrap_or_else(Utc::now);

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

#[cfg(test)]
mod tests {
    use super::WebScraperAdapter;
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

    #[test]
    fn parse_version_from_filename_extracts_semver_triplet() {
        let version = WebScraperAdapter::parse_version_from_filename("tool-v1.4.9-linux.tar.gz")
            .expect("parsed version");
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 4);
        assert_eq!(version.patch, 9);
    }

    #[tokio::test]
    async fn get_latest_release_selects_assets_for_latest_detected_version() {
        let html = r#"
            <html><body>
                <a href="/tool-v1.9.0-linux.tar.gz">old</a>
                <a href="/tool-v1.10.0-linux.tar.gz">new</a>
                <a href="/tool-v1.10.0-linux.sha256">checksum</a>
            </body></html>
        "#
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

        let adapter = WebScraperAdapter::new(HttpClient::new().expect("http client"));
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
    async fn conditional_latest_release_returns_none_on_not_modified() {
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "GET");
            http_response("HTTP/1.1 304 Not Modified", &[("Connection", "close")], "")
        });
        let adapter = WebScraperAdapter::new(HttpClient::new().expect("http client"));
        let release = adapter
            .get_latest_release_if_modified_since(&server, Some(Utc::now()))
            .await
            .expect("conditional release");
        assert!(release.is_none());
    }
}
