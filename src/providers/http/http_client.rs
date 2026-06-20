use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, header};
use std::collections::HashSet;
use std::path::Path;

use crate::models::common::enums::Filetype;
use crate::models::upstream::DownloadConfig;
use crate::providers::download_handler;
use crate::utils::filename_parser::parse_filetype;

#[derive(Debug, Clone)]
pub struct HttpAssetInfo {
    pub download_url: String,
    pub name: String,
    pub size: u64,
    pub last_modified: Option<DateTime<Utc>>,
    pub etag: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ConditionalProbeResult {
    NotModified,
    Asset(HttpAssetInfo),
}

#[derive(Debug, Clone)]
pub enum ConditionalDiscoveryResult {
    NotModified,
    Assets(Vec<HttpAssetInfo>),
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    client: Client,
    download_config: DownloadConfig,
}

impl HttpClient {
    fn format_http_date(dt: DateTime<Utc>) -> String {
        dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }

    fn add_if_modified_since(
        mut request: reqwest::RequestBuilder,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> reqwest::RequestBuilder {
        if let Some(ts) = last_upgraded {
            request = request.header(header::IF_MODIFIED_SINCE, Self::format_http_date(ts));
        }
        request
    }

    fn parse_last_modified(value: Option<&header::HeaderValue>) -> Option<DateTime<Utc>> {
        let raw = value?.to_str().ok()?;
        DateTime::parse_from_rfc2822(raw)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn parse_etag(value: Option<&header::HeaderValue>) -> Option<String> {
        value
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .map(|s| s.trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
    }

    fn attribute_has_boundary(html: &str, index: usize, attribute: &str) -> bool {
        let bytes = html.as_bytes();
        let valid_start = index == 0
            || bytes
                .get(index.saturating_sub(1))
                .map(|b| !b.is_ascii_alphanumeric() && *b != b'-')
                .unwrap_or(true);
        let end = index + attribute.len();
        let valid_end = bytes
            .get(end)
            .map(|b| *b == b'=' || b.is_ascii_whitespace())
            .unwrap_or(false);

        valid_start && valid_end
    }

    pub fn new(download_config: DownloadConfig) -> Result<Self> {
        let mut headers = header::HeaderMap::new();

        let user_agent = format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_str(&user_agent)
                .context("Failed to create user agent header")?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            download_config,
        })
    }

    /// Normalize provider inputs so bare hosts/slugs become HTTPS URLs.
    pub fn normalize_url(url_or_slug: &str) -> String {
        let raw = url_or_slug.trim();
        if raw.starts_with("http://") || raw.starts_with("https://") {
            raw.to_string()
        } else {
            format!("https://{}", raw)
        }
    }

    /// Extract likely download URL attribute values from HTML without a full DOM parser.
    fn extract_link_values(html: &str) -> Vec<String> {
        let attributes = [
            "href",
            "src",
            "data-href",
            "data-url",
            "data-download",
            "data-download-url",
        ];
        let mut values = Vec::new();
        let lower = html.to_lowercase();
        let bytes = lower.as_bytes();
        let mut i = 0_usize;

        while i < bytes.len() {
            let Some((attribute, attr_offset)) = attributes
                .iter()
                .filter_map(|attribute| {
                    lower[i..]
                        .find(attribute)
                        .map(|offset| (*attribute, offset))
                })
                .min_by(|(left_attr, left_offset), (right_attr, right_offset)| {
                    left_offset
                        .cmp(right_offset)
                        .then_with(|| right_attr.len().cmp(&left_attr.len()))
                })
            else {
                break;
            };

            i += attr_offset;
            if !Self::attribute_has_boundary(&lower, i, attribute) {
                i += 1;
                continue;
            }

            let mut j = i + attribute.len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= bytes.len() || bytes[j] != b'=' {
                i += 1;
                continue;
            }
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }

            let quote = bytes[j];
            if quote == b'"' || quote == b'\'' {
                let start = j + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != quote {
                    end += 1;
                }
                if end <= html.len() && start <= end {
                    let href = html[start..end].trim();
                    if !href.is_empty() {
                        values.push(href.to_string());
                    }
                }
                i = end.saturating_add(1);
                continue;
            }

            let start = j;
            let mut end = start;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace() && bytes[end] != b'>' {
                end += 1;
            }
            if end <= html.len() && start < end {
                let href = html[start..end].trim();
                if !href.is_empty() {
                    values.push(href.to_string());
                }
            }

            i = j.saturating_add(1);
        }

        values
    }

    fn to_asset_info(url: &str, headers: &header::HeaderMap) -> HttpAssetInfo {
        HttpAssetInfo {
            name: Self::file_name_from_url(url),
            download_url: url.to_string(),
            size: headers
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
            last_modified: Self::parse_last_modified(headers.get(header::LAST_MODIFIED)),
            etag: Self::parse_etag(headers.get(header::ETAG)),
        }
    }

    /// Convert discovered links into unique, non-checksum HTTP assets.
    fn extract_assets_from_html(
        base: &reqwest::Url,
        html: &str,
        page_headers: &header::HeaderMap,
    ) -> Vec<HttpAssetInfo> {
        let hrefs = Self::extract_link_values(html);
        let page_last_modified = Self::parse_last_modified(page_headers.get(header::LAST_MODIFIED));
        let page_etag = Self::parse_etag(page_headers.get(header::ETAG));

        let mut seen = HashSet::new();
        let mut assets = Vec::new();
        for href in hrefs {
            if href.starts_with('#')
                || href.starts_with("javascript:")
                || href.starts_with("mailto:")
                || href.starts_with("tel:")
            {
                continue;
            }

            let Ok(joined) = base.join(&href) else {
                continue;
            };
            if joined.scheme() != "http" && joined.scheme() != "https" {
                continue;
            }

            let joined_str = joined.to_string();
            let name = Self::file_name_from_url(&joined_str);
            if name.is_empty() {
                continue;
            }

            if parse_filetype(&name) == Filetype::Checksum {
                continue;
            }

            if seen.insert(joined_str.clone()) {
                assets.push(HttpAssetInfo {
                    download_url: joined_str,
                    name,
                    size: 0,
                    last_modified: page_last_modified,
                    etag: page_etag.clone(),
                });
            }
        }
        assets
    }

    /// Discover downloadable assets from an HTTP endpoint with optional
    /// `If-Modified-Since` behavior.
    pub async fn discover_assets_if_modified_since(
        &self,
        url_or_slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<ConditionalDiscoveryResult> {
        let url = Self::normalize_url(url_or_slug);
        let response = Self::add_if_modified_since(self.client.get(&url), last_upgraded)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(ConditionalDiscoveryResult::NotModified);
        }

        response
            .error_for_status_ref()
            .context(format!("HTTP server returned error for {}", url))?;

        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        let response_headers = response.headers().clone();

        if !content_type.contains("text/html") {
            return Ok(ConditionalDiscoveryResult::Assets(vec![
                Self::to_asset_info(&final_url, response.headers()),
            ]));
        }

        let base = reqwest::Url::parse(&final_url)
            .context(format!("Failed to parse URL '{}'", final_url))?;
        let body = response.text().await.context("Failed to read HTML body")?;
        let assets = Self::extract_assets_from_html(&base, &body, &response_headers);

        if assets.is_empty() {
            Ok(ConditionalDiscoveryResult::Assets(vec![
                Self::to_asset_info(&final_url, &response_headers),
            ]))
        } else {
            Ok(ConditionalDiscoveryResult::Assets(assets))
        }
    }

    /// Derive a filename from URL path segments with a safe fallback.
    pub fn file_name_from_url(url: &str) -> String {
        let without_fragment = url.split('#').next().unwrap_or(url);
        let without_query = without_fragment
            .split('?')
            .next()
            .unwrap_or(without_fragment);
        let candidate = without_query.rsplit('/').next().unwrap_or("").trim();

        if candidate.is_empty() {
            "download.bin".to_string()
        } else {
            candidate.to_string()
        }
    }

    pub async fn probe_asset(&self, url_or_slug: &str) -> Result<HttpAssetInfo> {
        match self
            .probe_asset_if_modified_since(url_or_slug, None)
            .await?
        {
            ConditionalProbeResult::NotModified => {
                bail!("Unexpected 304 Not Modified response without conditional timestamp")
            }
            ConditionalProbeResult::Asset(asset) => Ok(asset),
        }
    }

    pub async fn probe_asset_if_modified_since(
        &self,
        url_or_slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<ConditionalProbeResult> {
        let url = Self::normalize_url(url_or_slug);

        let head_resp = Self::add_if_modified_since(self.client.head(&url), last_upgraded)
            .send()
            .await;

        let (size, last_modified, etag) = match head_resp {
            Ok(resp) if resp.status() == StatusCode::NOT_MODIFIED => {
                return Ok(ConditionalProbeResult::NotModified);
            }
            Ok(resp) if resp.status().is_success() => {
                let last_modified =
                    Self::parse_last_modified(resp.headers().get(header::LAST_MODIFIED));
                let etag = Self::parse_etag(resp.headers().get(header::ETAG));
                (resp.content_length().unwrap_or(0), last_modified, etag)
            }
            Ok(resp)
                if resp.status() == StatusCode::METHOD_NOT_ALLOWED
                    || resp.status() == StatusCode::NOT_IMPLEMENTED =>
            {
                let get_resp = Self::add_if_modified_since(self.client.get(&url), last_upgraded)
                    .send()
                    .await
                    .context(format!("Failed to send request to {}", url))?;

                if get_resp.status() == StatusCode::NOT_MODIFIED {
                    return Ok(ConditionalProbeResult::NotModified);
                }

                get_resp
                    .error_for_status_ref()
                    .context(format!("HTTP server returned error for {}", url))?;
                let last_modified =
                    Self::parse_last_modified(get_resp.headers().get(header::LAST_MODIFIED));
                let etag = Self::parse_etag(get_resp.headers().get(header::ETAG));
                (get_resp.content_length().unwrap_or(0), last_modified, etag)
            }
            Ok(resp) => {
                bail!("HTTP server returned {} for {}", resp.status(), url);
            }
            Err(_) => {
                let get_resp = Self::add_if_modified_since(self.client.get(&url), last_upgraded)
                    .send()
                    .await
                    .context(format!("Failed to send request to {}", url))?;

                if get_resp.status() == StatusCode::NOT_MODIFIED {
                    return Ok(ConditionalProbeResult::NotModified);
                }

                get_resp
                    .error_for_status_ref()
                    .context(format!("HTTP server returned error for {}", url))?;
                let last_modified =
                    Self::parse_last_modified(get_resp.headers().get(header::LAST_MODIFIED));
                let etag = Self::parse_etag(get_resp.headers().get(header::ETAG));
                (get_resp.content_length().unwrap_or(0), last_modified, etag)
            }
        };

        Ok(ConditionalProbeResult::Asset(HttpAssetInfo {
            name: Self::file_name_from_url(&url),
            download_url: url,
            size,
            last_modified,
            etag,
        }))
    }

    pub async fn download_file<F>(
        &self,
        url: &str,
        destination: &Path,
        progress: &mut Option<F>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
    {
        download_handler::download_file(
            &self.client,
            url,
            destination,
            progress,
            self.download_config,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{ConditionalDiscoveryResult, ConditionalProbeResult, HttpClient};
    use chrono::Utc;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

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

    fn temp_file_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-http-test-{name}-{nanos}.bin"))
    }

    fn cleanup_file(path: &Path) -> io::Result<()> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    #[test]
    fn normalize_url_and_file_name_from_url_behave_as_expected() {
        assert_eq!(
            HttpClient::normalize_url("example.com/a"),
            "https://example.com/a"
        );
        assert_eq!(
            HttpClient::normalize_url("http://example.com/a"),
            "http://example.com/a"
        );

        assert_eq!(
            HttpClient::file_name_from_url("https://x.invalid/path/tool.tar.gz?x=1#frag"),
            "tool.tar.gz"
        );
        assert_eq!(
            HttpClient::file_name_from_url("https://x.invalid/path/"),
            "download.bin"
        );
    }

    #[tokio::test]
    async fn discover_assets_extracts_and_filters_html_links() {
        let html = r##"
                <html><body>
                    <a href="tool-v1.2.3-linux.tar.gz">main</a>
                    <a href="/downloads/tool-v1.2.3-linux.tar.gz">duplicate</a>
                    <a href="tool-v1.2.3.sha256">checksum</a>
                    <a href="mailto:test@example.com">mail</a>
                    <a href="#anchor">anchor</a>
                    <a href="https://example.invalid/tool-v1.2.3-macos.zip">mac</a>
                    <button data-download-url="/tool-v1.2.3-windows.zip">win</button>
                </body></html>
            "##;
        let body = html.to_string();
        let last_modified = "Tue, 10 Feb 2026 15:04:05 GMT".to_string();
        let server = spawn_test_server(1, move |_, _| {
            http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Content-Type", "text/html"),
                    ("Last-Modified", &last_modified),
                    ("Content-Length", &body.len().to_string()),
                    ("Connection", "close"),
                ],
                &body,
            )
        });
        let client = HttpClient::new(Default::default()).expect("client");

        let result = client
            .discover_assets_if_modified_since(&server, None)
            .await
            .expect("discover assets");

        match result {
            ConditionalDiscoveryResult::NotModified => panic!("unexpected not modified"),
            ConditionalDiscoveryResult::Assets(assets) => {
                assert_eq!(assets.len(), 4);
                assert!(
                    assets
                        .iter()
                        .any(|a| a.name.ends_with("tool-v1.2.3-linux.tar.gz"))
                );
                assert!(assets.iter().all(|a| !a.name.ends_with(".sha256")));
                assert!(assets.iter().all(|a| a.last_modified.is_some()));
                assert!(
                    assets
                        .iter()
                        .any(|a| a.name.ends_with("tool-v1.2.3-windows.zip"))
                );
            }
        }
    }

    #[test]
    fn extract_link_values_accepts_spaced_and_unquoted_attributes() {
        let html = r#"
            <a href = "tool-a.zip">quoted with spaces</a>
            <a HREF='tool-b.tar.gz'>uppercase single quoted</a>
            <a href=tool-c.7z>unquoted</a>
            <button data-download-url = /tool-d.zip>data attr</button>
        "#;

        let values = HttpClient::extract_link_values(html);

        assert!(values.contains(&"tool-a.zip".to_string()));
        assert!(values.contains(&"tool-b.tar.gz".to_string()));
        assert!(values.contains(&"tool-c.7z".to_string()));
        assert!(values.contains(&"/tool-d.zip".to_string()));
    }

    #[tokio::test]
    async fn probe_asset_if_modified_since_returns_not_modified_on_304() {
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "HEAD");
            http_response("HTTP/1.1 304 Not Modified", &[("Connection", "close")], "")
        });
        let client = HttpClient::new(Default::default()).expect("client");

        let result = client
            .probe_asset_if_modified_since(&server, Some(Utc::now()))
            .await
            .expect("probe");
        assert!(matches!(result, ConditionalProbeResult::NotModified));
    }

    #[tokio::test]
    async fn probe_asset_if_modified_since_falls_back_to_get_on_405_head() {
        let last_modified = "Tue, 10 Feb 2026 15:04:05 GMT".to_string();
        let etag = "\"abc123\"".to_string();
        let server = spawn_test_server(2, move |method, _| match method {
            "HEAD" => http_response(
                "HTTP/1.1 405 Method Not Allowed",
                &[("Connection", "close"), ("Content-Length", "0")],
                "",
            ),
            "GET" => http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Connection", "close"),
                    ("Content-Length", "11"),
                    ("Last-Modified", &last_modified),
                    ("ETag", &etag),
                ],
                "hello world",
            ),
            _ => http_response(
                "HTTP/1.1 500 Internal Server Error",
                &[("Connection", "close"), ("Content-Length", "0")],
                "",
            ),
        });
        let client = HttpClient::new(Default::default()).expect("client");

        let result = client
            .probe_asset_if_modified_since(&format!("{server}/tool-v2.3.4.tar.gz"), None)
            .await
            .expect("probe fallback");

        match result {
            ConditionalProbeResult::NotModified => panic!("unexpected not modified"),
            ConditionalProbeResult::Asset(asset) => {
                assert_eq!(asset.size, 11);
                assert_eq!(asset.etag.as_deref(), Some("abc123"));
                assert!(asset.last_modified.is_some());
                assert_eq!(asset.name, "tool-v2.3.4.tar.gz");
            }
        }
    }

    #[tokio::test]
    async fn download_file_writes_bytes_and_reports_progress() {
        let body = "stream-body-data".to_string();
        let len = body.len().to_string();
        let body_for_server = body.clone();
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "GET");
            http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Connection", "close"),
                    ("Content-Type", "application/octet-stream"),
                    ("Content-Length", &len),
                ],
                &body_for_server,
            )
        });
        let client = HttpClient::new(Default::default()).expect("client");
        let output = temp_file_path("download");
        let mut progress = Vec::new();
        let mut cb = Some(|downloaded: u64, total: u64| {
            progress.push((downloaded, total));
        });

        client
            .download_file(&server, &output, &mut cb)
            .await
            .expect("download file");

        assert_eq!(fs::read_to_string(&output).expect("read output file"), body);
        assert!(!progress.is_empty());
        assert_eq!(
            progress.last().copied().expect("final progress"),
            (body.len() as u64, body.len() as u64)
        );

        cleanup_file(&output).expect("cleanup output file");
    }
}
