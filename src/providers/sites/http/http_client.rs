use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, header};
use std::path::Path;

use crate::models::upstream::config::DownloadConfig;
use crate::providers::shared::download_handler;

use crate::providers::shared::http_status;

#[derive(Debug, Clone)]
pub struct HttpAssetInfo {
    pub download_url: String,
    pub name: String,
    pub size: u64,
    pub last_modified: Option<DateTime<Utc>>,
    pub etag: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ConditionalProbeResult {
    NotModified,
    Asset(HttpAssetInfo),
}

#[derive(Debug, Clone)]
pub enum ConditionalDocumentResult {
    NotModified,
    Document(HttpDocument),
}

#[derive(Debug, Clone)]
pub struct HttpDocument {
    pub url: String,
    pub content_type: String,
    pub headers: header::HeaderMap,
    pub body: Vec<u8>,
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

    pub fn asset_info(url: &str, headers: &header::HeaderMap) -> HttpAssetInfo {
        HttpAssetInfo {
            name: Self::file_name_from_headers_or_url(url, headers),
            download_url: url.to_string(),
            size: headers
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
            last_modified: Self::parse_last_modified(headers.get(header::LAST_MODIFIED)),
            etag: Self::parse_etag(headers.get(header::ETAG)),
            content_type: headers
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_ascii_lowercase),
        }
    }

    fn file_name_from_headers_or_url(url: &str, headers: &header::HeaderMap) -> String {
        let filename = headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .and_then(Self::filename_from_content_disposition);
        filename.unwrap_or_else(|| Self::file_name_from_url(url))
    }

    fn filename_from_content_disposition(value: &str) -> Option<String> {
        value.split(';').skip(1).find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            let value = value.trim().trim_matches('"');
            if key.eq_ignore_ascii_case("filename") {
                return (!value.is_empty()).then(|| value.to_string());
            }
            if key.eq_ignore_ascii_case("filename*") {
                let encoded = value.rsplit("''").next()?;
                return Some(Self::percent_decode(encoded));
            }
            None
        })
    }

    fn percent_decode(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%'
                && index + 2 < bytes.len()
                && let (Some(high), Some(low)) = (
                    char::from(bytes[index + 1]).to_digit(16),
                    char::from(bytes[index + 2]).to_digit(16),
                )
            {
                decoded.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
            decoded.push(bytes[index]);
            index += 1;
        }
        String::from_utf8_lossy(&decoded).into_owned()
    }

    /// Fetch a page or direct artifact. HTML interpretation belongs to the
    /// scraper provider; this client only owns HTTP transport and metadata.
    pub async fn fetch_document_if_modified_since(
        &self,
        url_or_slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<ConditionalDocumentResult> {
        let url = Self::normalize_url(url_or_slug);
        let response = Self::add_if_modified_since(self.client.get(&url), last_upgraded)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(ConditionalDocumentResult::NotModified);
        }

        http_status::error_for_status(&response, "HTTP server", &url)?;

        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let response_headers = response.headers().clone();

        let body = response
            .bytes()
            .await
            .context("Failed to read HTTP response body")?;
        Ok(ConditionalDocumentResult::Document(HttpDocument {
            url: final_url,
            content_type,
            headers: response_headers,
            body: body.to_vec(),
        }))
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

        let (url, headers) = match head_resp {
            Ok(resp) if resp.status() == StatusCode::NOT_MODIFIED => {
                return Ok(ConditionalProbeResult::NotModified);
            }
            Ok(resp) if resp.status().is_success() => {
                (resp.url().to_string(), resp.headers().clone())
            }
            Ok(resp)
                if resp.status() == StatusCode::METHOD_NOT_ALLOWED
                    || resp.status() == StatusCode::NOT_IMPLEMENTED =>
            {
                let get_resp = Self::add_if_modified_since(
                    self.client.get(&url).header(header::RANGE, "bytes=0-0"),
                    last_upgraded,
                )
                .send()
                .await
                .context(format!("Failed to send request to {}", url))?;

                if get_resp.status() == StatusCode::NOT_MODIFIED {
                    return Ok(ConditionalProbeResult::NotModified);
                }

                http_status::error_for_status(&get_resp, "HTTP server", &url)?;
                (get_resp.url().to_string(), get_resp.headers().clone())
            }
            Ok(resp) => {
                if let Some(message) = http_status::rate_limit_message(
                    resp.status(),
                    resp.headers(),
                    "HTTP server",
                    &url,
                ) {
                    bail!("{message}");
                }

                bail!("HTTP server returned {} for {}", resp.status(), url);
            }
            Err(_) => {
                let get_resp = Self::add_if_modified_since(
                    self.client.get(&url).header(header::RANGE, "bytes=0-0"),
                    last_upgraded,
                )
                .send()
                .await
                .context(format!("Failed to send request to {}", url))?;

                if get_resp.status() == StatusCode::NOT_MODIFIED {
                    return Ok(ConditionalProbeResult::NotModified);
                }

                http_status::error_for_status(&get_resp, "HTTP server", &url)?;
                (get_resp.url().to_string(), get_resp.headers().clone())
            }
        };

        Ok(ConditionalProbeResult::Asset(Self::asset_info(
            &url, &headers,
        )))
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
    use super::{ConditionalDocumentResult, ConditionalProbeResult, HttpClient};
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

    #[test]
    fn content_disposition_filename_overrides_opaque_download_url() {
        let headers = reqwest::header::HeaderMap::from_iter([(
            reqwest::header::CONTENT_DISPOSITION,
            reqwest::header::HeaderValue::from_static(
                "attachment; filename*=UTF-8''tool%20v1.2.3.zip",
            ),
        )]);

        assert_eq!(
            HttpClient::asset_info("https://example.invalid/download?id=42", &headers).name,
            "tool v1.2.3.zip"
        );
    }

    #[tokio::test]
    async fn fetch_document_preserves_html_response() {
        let html = include_str!("../../../../tests/fixtures/providers/http/discovery-links.html");
        let body = html.to_string();
        let response_body = body.clone();
        let last_modified = "Tue, 10 Feb 2026 15:04:05 GMT".to_string();
        let server = spawn_test_server(1, move |_, _| {
            http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Content-Type", "text/html"),
                    ("Last-Modified", &last_modified),
                    ("Content-Length", &response_body.len().to_string()),
                    ("Connection", "close"),
                ],
                &response_body,
            )
        });

        let client = HttpClient::new(Default::default()).expect("client");

        let result = client
            .fetch_document_if_modified_since(&server, None)
            .await
            .expect("fetch");
        let ConditionalDocumentResult::Document(document) = result else {
            panic!("unexpected not modified");
        };
        assert!(document.content_type.contains("text/html"));
        assert_eq!(document.url, format!("{server}/"));
        assert_eq!(document.body, body.as_bytes());
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
