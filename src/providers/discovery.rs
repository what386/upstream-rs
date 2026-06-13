use anyhow::{Result, anyhow};
use reqwest::Url;

use crate::{
    models::{
        common::enums::{Channel, Filetype, Provider},
        provider::Release,
        upstream::Package,
    },
    providers::{asset_selector::AssetCandidate, provider_manager::ProviderManager},
    utils::filename_parser::parse_filetype,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceKind {
    Repository,
    ForgeUrl,
    DirectAsset,
    DownloadPage,
}

#[derive(Debug, Clone)]
pub struct DiscoveredSource {
    pub original: String,
    pub repo_slug: String,
    pub provider: Provider,
    pub base_url: Option<String>,
    pub kind: SourceKind,
}

#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub source: DiscoveredSource,
    pub releases: Vec<Release>,
    pub candidates: Vec<AssetCandidate>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryRequest {
    pub source: String,
    pub channel: Channel,
    pub package_name: String,
    pub filetype: Filetype,
    pub match_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
    pub base_url_override: Option<String>,
    pub limit: u32,
}

impl DiscoveryResult {
    pub fn recommended_candidate(&self) -> Option<&AssetCandidate> {
        self.candidates.first()
    }

    pub fn is_ambiguous(&self) -> bool {
        let Some(top) = self.candidates.first() else {
            return false;
        };
        let Some(next) = self.candidates.get(1) else {
            return false;
        };

        next.score >= top.score.saturating_sub(20)
    }
}

impl ProviderManager {
    pub async fn discover_source(&self, request: DiscoveryRequest) -> Result<DiscoveryResult> {
        let mut discovered = infer_source(&request.source)?;
        if let Some(base_url) = request.base_url_override.as_deref() {
            discovered.base_url = Some(base_url.to_string());
        }

        let mut releases = self
            .get_releases(
                &discovered.repo_slug,
                &discovered.provider,
                Some(request.limit),
                Some(request.limit),
                discovered.base_url.as_deref(),
            )
            .await?;

        releases = filter_releases_by_channel(releases, &request.channel);
        releases.sort_by(|a, b| b.version.cmp(&a.version));

        let probe_package = Package::with_defaults(
            request.package_name,
            discovered.repo_slug.clone(),
            request.filetype,
            request.match_pattern,
            request.exclude_pattern,
            request.channel,
            discovered.provider.clone(),
            discovered.base_url.clone(),
        );

        let candidates = releases
            .first()
            .map(|release| self.get_candidate_assets(release, &probe_package))
            .transpose()?
            .unwrap_or_default();

        Ok(DiscoveryResult {
            source: discovered,
            releases,
            candidates,
        })
    }
}

pub fn infer_source(source: &str) -> Result<DiscoveredSource> {
    let original = source.trim().to_string();
    if original.is_empty() {
        return Err(anyhow!("Source cannot be empty"));
    }

    if let Ok(url) = Url::parse(&original) {
        return infer_url_source(&original, &url);
    }

    if looks_like_repo_slug(&original) {
        return Ok(DiscoveredSource {
            original,
            repo_slug: source.trim_matches('/').to_string(),
            provider: Provider::Github,
            base_url: None,
            kind: SourceKind::Repository,
        });
    }

    Ok(DiscoveredSource {
        original: original.clone(),
        repo_slug: original,
        provider: Provider::WebScraper,
        base_url: None,
        kind: SourceKind::DownloadPage,
    })
}

pub fn normalize_source_for_provider(
    source: &str,
    provider: &Provider,
    base_url: Option<&str>,
) -> String {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Ok(url) = Url::parse(trimmed) else {
        return trimmed.trim_matches('/').to_string();
    };

    let host = url.host_str().unwrap_or("").to_lowercase();
    let segments: Vec<&str> = url
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();

    match provider {
        Provider::Github => {
            if (host == "github.com" || host == "www.github.com")
                && let Some(slug) = owner_repo_slug(&segments)
            {
                return slug;
            }
        }
        Provider::Gitlab => {
            if is_gitlab_host(&host)
                && let Some(slug) = gitlab_slug(&segments)
            {
                return slug;
            }
            if let Some(base) = base_url
                && let Ok(base_url_parsed) = Url::parse(base)
                && same_host(&url, &base_url_parsed)
                && let Some(slug) = gitlab_slug(&segments)
            {
                return slug;
            }
        }
        Provider::Gitea => {
            if (host == "gitea.com"
                || host == "www.gitea.com"
                || host == "codeberg.org"
                || host == "www.codeberg.org")
                && let Some(slug) = owner_repo_slug(&segments)
            {
                return slug;
            }
            if let Some(base) = base_url
                && let Ok(base_url_parsed) = Url::parse(base)
                && same_host(&url, &base_url_parsed)
                && let Some(slug) = owner_repo_slug(&segments)
            {
                return slug;
            }
        }
        Provider::Direct | Provider::WebScraper => {}
    }

    trimmed.to_string()
}

fn infer_url_source(original: &str, url: &Url) -> Result<DiscoveredSource> {
    let host = url.host_str().unwrap_or("").to_lowercase();
    let segments: Vec<&str> = url
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();

    if (host == "github.com" || host == "www.github.com")
        && let Some(slug) = owner_repo_slug(&segments)
    {
        return Ok(DiscoveredSource {
            original: original.to_string(),
            repo_slug: slug,
            provider: Provider::Github,
            base_url: None,
            kind: SourceKind::ForgeUrl,
        });
    }

    if is_gitlab_host(&host)
        && let Some(slug) = gitlab_slug(&segments)
    {
        return Ok(DiscoveredSource {
            original: original.to_string(),
            repo_slug: slug,
            provider: Provider::Gitlab,
            base_url: gitlab_base_url(url, &host),
            kind: SourceKind::ForgeUrl,
        });
    }

    if (host == "gitea.com"
        || host == "www.gitea.com"
        || host == "codeberg.org"
        || host == "www.codeberg.org")
        && let Some(slug) = owner_repo_slug(&segments)
    {
        return Ok(DiscoveredSource {
            original: original.to_string(),
            repo_slug: slug,
            provider: Provider::Gitea,
            base_url: Some(format!("{}://{}", url.scheme(), host)),
            kind: SourceKind::ForgeUrl,
        });
    }

    if is_direct_asset_url(url) {
        return Ok(DiscoveredSource {
            original: original.to_string(),
            repo_slug: original.to_string(),
            provider: Provider::Direct,
            base_url: None,
            kind: SourceKind::DirectAsset,
        });
    }

    Ok(DiscoveredSource {
        original: original.to_string(),
        repo_slug: original.to_string(),
        provider: Provider::WebScraper,
        base_url: None,
        kind: SourceKind::DownloadPage,
    })
}

fn looks_like_repo_slug(value: &str) -> bool {
    let parts: Vec<&str> = value.split('/').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && !part.contains(char::is_whitespace))
}

fn owner_repo_slug(segments: &[&str]) -> Option<String> {
    if segments.len() < 2 {
        return None;
    }

    let owner = segments[0];
    let repo = segments[1].trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    Some(format!("{owner}/{repo}"))
}

fn gitlab_slug(segments: &[&str]) -> Option<String> {
    let stop_markers = ["-", "releases", "downloads", "packages"];
    let parts: Vec<&str> = segments
        .iter()
        .copied()
        .take_while(|segment| !stop_markers.contains(segment))
        .collect();

    if parts.len() < 2 {
        return None;
    }

    Some(parts.join("/"))
}

fn is_gitlab_host(host: &str) -> bool {
    let normalized = host.strip_prefix("www.").unwrap_or(host);
    normalized == "gitlab.com" || normalized.starts_with("gitlab.")
}

fn gitlab_base_url(url: &Url, host: &str) -> Option<String> {
    if matches!(host, "gitlab.com" | "www.gitlab.com") {
        None
    } else {
        Some(format!("{}://{}", url.scheme(), host))
    }
}

fn same_host(a: &Url, b: &Url) -> bool {
    match (a.host_str(), b.host_str()) {
        (Some(ha), Some(hb)) => ha.eq_ignore_ascii_case(hb),
        _ => false,
    }
}

fn is_direct_asset_url(url: &Url) -> bool {
    let filename = url
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .unwrap_or("");

    !matches!(
        parse_filetype(filename),
        Filetype::Binary | Filetype::Checksum | Filetype::Auto
    )
}

fn filter_releases_by_channel(mut releases: Vec<Release>, channel: &Channel) -> Vec<Release> {
    match channel {
        Channel::Stable => {
            releases.retain(|r| !r.is_prerelease && !ProviderManager::is_nightly_release(&r.tag))
        }
        Channel::Preview => releases.retain(ProviderManager::is_preview_release),
        Channel::Nightly => releases.retain(|r| ProviderManager::is_nightly_release(&r.tag)),
    }
    releases
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveredSource, DiscoveryResult, SourceKind, infer_source, normalize_source_for_provider,
    };
    use crate::models::{
        common::{Version, enums::Provider},
        provider::{Asset, Release},
    };
    use crate::providers::asset_selector::AssetCandidate;
    use chrono::Utc;

    #[test]
    fn infer_source_keeps_owner_repo_as_github() {
        let source = infer_source("BurntSushi/ripgrep").expect("infer source");

        assert_eq!(source.provider, Provider::Github);
        assert_eq!(source.repo_slug, "BurntSushi/ripgrep");
        assert_eq!(source.kind, SourceKind::Repository);
    }

    #[test]
    fn infer_source_normalizes_github_release_urls() {
        let source =
            infer_source("https://github.com/sharkdp/fd/releases/latest").expect("infer source");

        assert_eq!(source.provider, Provider::Github);
        assert_eq!(source.repo_slug, "sharkdp/fd");
        assert_eq!(source.kind, SourceKind::ForgeUrl);
    }

    #[test]
    fn infer_source_normalizes_plain_github_repo_urls() {
        let source = infer_source("https://github.com/sharkdp/bat").expect("infer source");
        assert_eq!(source.provider, Provider::Github);
        assert_eq!(source.repo_slug, "sharkdp/bat");
        assert_eq!(source.kind, SourceKind::ForgeUrl);
    }

    #[test]
    fn infer_source_normalizes_www_github_repo_urls() {
        let source = infer_source("https://www.github.com/sharkdp/bat/").expect("infer source");
        assert_eq!(source.provider, Provider::Github);
        assert_eq!(source.repo_slug, "sharkdp/bat");
        assert_eq!(source.kind, SourceKind::ForgeUrl);
    }

    #[test]
    fn infer_source_strips_git_suffix_for_repo_urls() {
        let source = infer_source("https://github.com/sharkdp/bat.git").expect("infer source");
        assert_eq!(source.provider, Provider::Github);
        assert_eq!(source.repo_slug, "sharkdp/bat");
        assert_eq!(source.kind, SourceKind::ForgeUrl);
    }

    #[test]
    fn infer_source_normalizes_self_hosted_gitlab_urls() {
        let source = infer_source("https://gitlab.futo.org/videostreaming/Grayjay.Desktop")
            .expect("infer source");

        assert_eq!(source.provider, Provider::Gitlab);
        assert_eq!(source.repo_slug, "videostreaming/Grayjay.Desktop");
        assert_eq!(source.base_url.as_deref(), Some("https://gitlab.futo.org"));
        assert_eq!(source.kind, SourceKind::ForgeUrl);
    }

    #[test]
    fn normalize_source_for_provider_extracts_slug_for_github_urls() {
        let normalized = normalize_source_for_provider(
            "https://github.com/sharkdp/bat",
            &Provider::Github,
            None,
        );
        assert_eq!(normalized, "sharkdp/bat");
    }

    #[test]
    fn normalize_source_for_provider_extracts_slug_for_self_hosted_gitlab_urls() {
        let normalized = normalize_source_for_provider(
            "https://gitlab.futo.org/videostreaming/Grayjay.Desktop",
            &Provider::Gitlab,
            None,
        );

        assert_eq!(normalized, "videostreaming/Grayjay.Desktop");
    }

    #[test]
    fn infer_source_normalizes_codeberg_urls_as_gitea() {
        let source =
            infer_source("https://codeberg.org/forgejo/forgejo/releases").expect("infer source");

        assert_eq!(source.provider, Provider::Gitea);
        assert_eq!(source.repo_slug, "forgejo/forgejo");
        assert_eq!(source.base_url.as_deref(), Some("https://codeberg.org"));
    }

    #[test]
    fn infer_source_detects_direct_assets() {
        let source =
            infer_source("https://example.invalid/download/tool-linux-x64.tar.gz").expect("infer");

        assert_eq!(source.provider, Provider::Direct);
        assert_eq!(source.kind, SourceKind::DirectAsset);
    }

    #[test]
    fn infer_source_uses_scraper_for_generic_pages() {
        let source = infer_source("https://example.invalid/downloads").expect("infer");

        assert_eq!(source.provider, Provider::WebScraper);
        assert_eq!(source.kind, SourceKind::DownloadPage);
    }

    #[test]
    fn discovery_result_marks_close_scores_as_ambiguous() {
        let release = Release {
            id: 1,
            tag: "v1.0.0".to_string(),
            name: "v1.0.0".to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets: Vec::new(),
            version: Version::new(1, 0, 0, false),
            published_at: Utc::now(),
        };
        let source = DiscoveredSource {
            original: "https://example.invalid/downloads".to_string(),
            repo_slug: "https://example.invalid/downloads".to_string(),
            provider: Provider::WebScraper,
            base_url: None,
            kind: SourceKind::DownloadPage,
        };
        let asset = Asset::new(
            "https://example.invalid/tool.tar.gz".to_string(),
            1,
            "tool.tar.gz".to_string(),
            1000,
            Utc::now(),
        );
        let result = DiscoveryResult {
            source,
            releases: vec![release],
            candidates: vec![
                AssetCandidate {
                    asset: asset.clone(),
                    score: 100,
                },
                AssetCandidate { asset, score: 80 },
            ],
        };

        assert!(result.is_ambiguous());
    }
}
