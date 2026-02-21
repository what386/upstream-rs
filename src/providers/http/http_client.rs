use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, header};
use std::collections::HashSet;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::models::common::enums::Filetype;
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

    pub fn new() -> Result<Self> {
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

        Ok(Self { client })
    }

    pub fn normalize_url(url_or_slug: &str) -> String {
        let raw = url_or_slug.trim();
        if raw.starts_with("http://") || raw.starts_with("https://") {
            raw.to_string()
        } else {
            format!("https://{}", raw)
        }
    }

    fn extract_hrefs(html: &str) -> Vec<String> {
        let mut hrefs = Vec::new();
        let lower = html.to_lowercase();
        let bytes = lower.as_bytes();
        let mut i = 0_usize;

        while i + 6 < bytes.len() {
            if &bytes[i..i + 5] != b"href=" {
                i += 1;
                continue;
            }
            let mut j = i + 5;
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
                        hrefs.push(href.to_string());
                    }
                }
                i = end.saturating_add(1);
                continue;
            }

            i = j.saturating_add(1);
        }

        hrefs
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

    fn extract_assets_from_html(base: &reqwest::Url, html: &str) -> Vec<HttpAssetInfo> {
        let hrefs = Self::extract_hrefs(html);

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
                    last_modified: None,
                    etag: None,
                });
            }
        }
        assets
    }

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
        let assets = Self::extract_assets_from_html(&base, &body);

        if assets.is_empty() {
            Ok(ConditionalDiscoveryResult::Assets(vec![
                Self::to_asset_info(&final_url, &response_headers),
            ]))
        } else {
            Ok(ConditionalDiscoveryResult::Assets(assets))
        }
    }

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
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context(format!("Failed to download from {}", url))?;

        response
            .error_for_status_ref()
            .context("Download request failed")?;

        let total_bytes = response.content_length().unwrap_or(0);

        let mut file = File::create(destination)
            .await
            .context(format!("Failed to create file at {:?}", destination))?;

        let mut stream = response.bytes_stream();
        let mut total_read: u64 = 0;

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read download chunk")?;

            file.write_all(&chunk)
                .await
                .context("Failed to write to file")?;

            total_read += chunk.len() as u64;

            if let Some(cb) = progress.as_mut() {
                cb(total_read, total_bytes);
            }
        }

        file.flush().await.context("Failed to flush file")?;

        if total_bytes > 0 && total_read != total_bytes {
            bail!(
                "Download size mismatch: expected {} bytes, got {} bytes",
                total_bytes,
                total_read
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ConditionalDiscoveryResult, ConditionalProbeResult, HttpClient};
    use chrono::Utc;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
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

    fn cleanup_file(path: &PathBuf) -> io::Result<()> {
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
            </body></html>
        "##;
        let body = html.to_string();
        let server = spawn_test_server(1, move |_, _| {
            http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Content-Type", "text/html"),
                    ("Content-Length", &body.len().to_string()),
                    ("Connection", "close"),
                ],
                &body,
            )
        });
        let client = HttpClient::new().expect("client");

        let result = client
            .discover_assets_if_modified_since(&server, None)
            .await
            .expect("discover assets");

        match result {
            ConditionalDiscoveryResult::NotModified => panic!("unexpected not modified"),
            ConditionalDiscoveryResult::Assets(assets) => {
                assert_eq!(assets.len(), 3);
                assert!(assets.iter().any(|a| a.name.ends_with("tool-v1.2.3-linux.tar.gz")));
                assert!(assets
                    .iter()
                    .all(|a| !a.name.ends_with(".sha256")));
            }
        }
    }

    #[tokio::test]
    async fn probe_asset_if_modified_since_returns_not_modified_on_304() {
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "HEAD");
            http_response(
                "HTTP/1.1 304 Not Modified",
                &[("Connection", "close")],
                "",
            )
        });
        let client = HttpClient::new().expect("client");

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
        let client = HttpClient::new().expect("client");

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
        let client = HttpClient::new().expect("client");
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
