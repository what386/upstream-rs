use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::http::http_client::{ConditionalProbeResult, HttpClient};

#[derive(Debug, Clone)]
pub struct DirectAdapter {
    client: HttpClient,
}

impl DirectAdapter {
    fn parse_version_from_filename(filename: &str) -> Option<Version> {
        Version::from_filename(filename).ok()
    }

    fn version_from_last_modified(dt: DateTime<Utc>) -> Version {
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
        bail!("Direct provider does not support tagged releases")
    }

    pub async fn get_latest_release(&self, slug: &str) -> Result<Release> {
        self.get_latest_release_if_modified_since(slug, None)
            .await?
            .ok_or_else(|| anyhow!("Unexpected not-modified response for direct provider"))
    }

    pub async fn get_latest_release_if_modified_since(
        &self,
        slug: &str,
        last_upgraded: Option<DateTime<Utc>>,
    ) -> Result<Option<Release>> {
        let probe = self
            .client
            .probe_asset_if_modified_since(slug, last_upgraded)
            .await?;
        let info = match probe {
            ConditionalProbeResult::NotModified => return Ok(None),
            ConditionalProbeResult::Asset(info) => info,
        };
        let published_at = info.last_modified.unwrap_or_else(Utc::now);
        let version = Self::parse_version_from_filename(&info.name)
            .or_else(|| info.last_modified.map(Self::version_from_last_modified))
            .unwrap_or_else(|| Version::new(0, 0, 0, false));

        let asset = Asset::new(
            info.download_url,
            1,
            info.name.clone(),
            info.size,
            published_at,
        );

        let release_name = if let Some(etag) = info.etag {
            format!("{} [{}]", info.name, etag)
        } else {
            info.name
        };

        Ok(Some(Release {
            id: 1,
            tag: "direct".to_string(),
            name: release_name,
            body: "Direct HTTP asset".to_string(),
            is_draft: false,
            is_prerelease: false,
            assets: vec![asset],
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
    use super::DirectAdapter;
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
        let version =
            DirectAdapter::parse_version_from_filename("tool-v1.2.3-linux-x86_64.tar.gz")
                .expect("parsed version");
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[tokio::test]
    async fn get_latest_release_builds_release_from_probe_metadata() {
        let etag = "\"etag-value\"".to_string();
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "HEAD");
            http_response(
                "HTTP/1.1 200 OK",
                &[
                    ("Connection", "close"),
                    ("Content-Length", "42"),
                    ("ETag", &etag),
                    ("Last-Modified", "Tue, 10 Feb 2026 15:04:05 GMT"),
                ],
                "",
            )
        });
        let adapter = DirectAdapter::new(HttpClient::new().expect("http client"));
        let release = adapter
            .get_latest_release(&format!("{server}/tool-v2.3.4.tar.gz"))
            .await
            .expect("release");

        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.version.major, 2);
        assert_eq!(release.version.minor, 3);
        assert_eq!(release.version.patch, 4);
        assert!(release.name.contains("etag-value"));
    }

    #[tokio::test]
    async fn conditional_latest_release_returns_none_on_not_modified() {
        let server = spawn_test_server(1, move |method, _| {
            assert_eq!(method, "HEAD");
            http_response("HTTP/1.1 304 Not Modified", &[("Connection", "close")], "")
        });
        let adapter = DirectAdapter::new(HttpClient::new().expect("http client"));

        let release = adapter
            .get_latest_release_if_modified_since(&server, Some(Utc::now()))
            .await
            .expect("conditional release");
        assert!(release.is_none());
    }
}
