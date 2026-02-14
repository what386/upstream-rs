use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::path::Path;

use crate::models::common::Version;
use crate::models::provider::{Asset, Release};
use crate::providers::http::http_client::HttpClient;

#[derive(Debug, Clone)]
pub struct WebScraperAdapter {
    client: HttpClient,
}

impl WebScraperAdapter {
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
        let mut infos = self.client.discover_assets(slug).await?;

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
        Ok(Release {
            id: 1,
            tag: "direct".to_string(),
            name: release_name,
            body: "Discovered from HTTP source".to_string(),
            is_draft: false,
            is_prerelease: false,
            assets,
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
