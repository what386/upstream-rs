use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use scraper::{Html, Selector};
use std::{collections::HashSet, path::Path};

use crate::{
    models::{
        common::{Version, enums::Filetype},
        provider::{Asset, Release},
    },
    providers::sites::{
        ReleaseProvider,
        http::{ConditionalDocumentResult, HttpAssetInfo, HttpClient},
    },
    utils::filenames::parser::parse_filetype,
};

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
    ) -> Vec<(Candidate, HttpAssetInfo)> {
        let mut validated = Vec::new();
        for candidate in candidates.into_iter().take(24) {
            let Ok(info) = self.client.probe_asset(&candidate.url).await else {
                continue;
            };
            if info.content_type.as_deref().is_some_and(|kind| {
                kind.contains("text/html") || kind.contains("application/xhtml")
            }) {
                continue;
            }
            if parse_filetype(&info.name) == Filetype::Checksum {
                continue;
            }
            let mut info = info;
            if info.last_modified.is_none() {
                info.last_modified = page_modified;
            }
            if info.etag.is_none() {
                info.etag = page_etag.clone();
            }
            validated.push((candidate, info));
        }
        validated
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
        let document = self
            .client
            .fetch_document_if_modified_since(slug, last_upgraded)
            .await?;
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
        let candidates = Self::extract_candidates(&base, &html);
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
            self.validate_candidates(candidates, page_modified, page_etag)
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
    use super::WebScraperAdapter;
    use crate::models::common::Version;

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

        assert_eq!(urls.len(), 2);
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
