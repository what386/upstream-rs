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

    /// Normalize provider inputs so bare hosts/slugs become HTTPS URLs.
    pub fn normalize_url(url_or_slug: &str) -> String {
        let raw = url_or_slug.trim();
        if raw.starts_with("http://") || raw.starts_with("https://") {
            raw.to_string()
        } else {
            format!("https://{}", raw)
        }
    }

    /// Extract raw `href` attribute values from HTML without a full DOM parser.
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

    /// Convert discovered links into unique, non-checksum HTTP assets.
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
        let assets = Self::extract_assets_from_html(&base, &body);

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
#[path = "../../../tests/providers/http/http_client.rs"]
mod tests;
