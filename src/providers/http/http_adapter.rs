use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::http::http_client::HttpClient;

#[derive(Debug, Clone)]
pub struct HttpAdapter {
    client: HttpClient,
}

impl HttpAdapter {
    fn parse_version_from_filename(filename: &str) -> Option<Version> {
        let lower = filename.to_lowercase();
        let mut sanitized = String::with_capacity(lower.len());
        for ch in lower.chars() {
            if ch.is_ascii_digit() || ch == '.' {
                sanitized.push(ch);
            } else {
                sanitized.push(' ');
            }
        }

        let mut best: Option<Version> = None;
        for token in sanitized.split_whitespace() {
            if token.chars().filter(|c| *c == '.').count() < 1 {
                continue;
            }
            let candidate = token.trim_matches('.');
            if let Ok(version) = Version::parse(candidate) {
                match &best {
                    Some(prev) if prev.cmp(&version).is_ge() => {}
                    _ => best = Some(version),
                }
            }
        }

        best
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
        let info = self.client.probe_asset(slug).await?;
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
            info.name.clone()
        };
        Ok(Release {
            id: 1,
            tag: "direct".to_string(),
            name: release_name,
            body: "Direct HTTP asset".to_string(),
            is_draft: false,
            is_prerelease: false,
            assets: vec![asset],
            version,
            published_at,
        })
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
