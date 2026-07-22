use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use futures_util::StreamExt;
use reqwest::{StatusCode, header};
use serde::{Deserialize, Serialize};

use crate::{
    application::commands::install,
    models::{
        common::enums::{Channel, Filetype, Provider, TrustMode},
        upstream::config::AppConfig,
    },
    output::{self, Status},
    utils::{filesystem::atomic_ops::write_atomic, static_paths::UpstreamPaths},
};

const SUPPORTED_INDEX_VERSION: u64 = 1;
const MAX_INDEX_BYTES: usize = 10 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct RegistryIndex {
    version: u64,
    packages: BTreeMap<String, RegistryPackage>,
}

#[derive(Debug, Deserialize)]
struct RegistryPackage {
    revision: u64,
    repo: String,
    provider: RegistryProvider,
    desktop: bool,
    trust: RegistryTrust,
    #[serde(default)]
    r#match: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RegistryProvider {
    Github,
    Gitlab,
    Gitea,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RegistryTrust {
    None,
    BestEffort,
    Checksum,
    Signature,
    All,
}

#[derive(Debug, Deserialize, Serialize)]
struct RegistryCacheMetadata {
    source_url: String,
    fetched_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_modified: Option<String>,
}

enum FetchOutcome {
    Updated,
    NotModified,
}

pub async fn run(
    name: String,
    fetch: bool,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let cache_file = paths.dirs.cache_dir.join("registry/index.min.json");
    let metadata_file = paths.dirs.cache_dir.join("registry/metadata.json");
    if fetch {
        let outcome =
            fetch_index(&app_config.registry.index_url, &cache_file, &metadata_file).await?;
        output::status_line(
            Status::Ok,
            "registry",
            match outcome {
                FetchOutcome::Updated => "index refreshed",
                FetchOutcome::NotModified => "index already current",
            },
        );
    }

    let index = load_cached_index(&cache_file, &metadata_file, &app_config.registry.index_url)?;
    let package = index.packages.get(&name).with_context(|| {
        format!(
            "Package '{name}' was not found in the registry. Use 'upstream add {name} --fetch' to refresh the local index."
        )
    })?;

    install::run(
        Some(name),
        package.repo.clone(),
        Filetype::Auto,
        None,
        None,
        Some(package.provider.provider()),
        None,
        Channel::Stable,
        joined_patterns(&package.r#match),
        joined_patterns(&package.exclude),
        package.desktop,
        package.trust.trust_mode(),
        dry_run,
        paths,
        app_config,
    )
    .await
}

async fn fetch_index(url: &str, cache_file: &Path, metadata_file: &Path) -> Result<FetchOutcome> {
    let existing_metadata = usable_cache_metadata(cache_file, metadata_file, url);
    let client = reqwest::Client::builder()
        .user_agent(format!(
            "{}/{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(FETCH_TIMEOUT)
        .build()
        .context("Failed to create registry HTTP client")?;
    let mut request = client.get(url).header(header::ACCEPT, "application/json");
    if let Some(metadata) = &existing_metadata {
        if let Some(etag) = &metadata.etag {
            request = request.header(header::IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &metadata.last_modified {
            request = request.header(header::IF_MODIFIED_SINCE, last_modified);
        }
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("Failed to fetch registry index from '{url}'"))?;

    if response.status() == StatusCode::NOT_MODIFIED {
        let mut metadata = existing_metadata
            .context("Registry server returned not modified, but no valid matching cache exists")?;
        metadata.fetched_at = Utc::now().to_rfc3339();
        write_cache_metadata(metadata_file, &metadata)?;
        return Ok(FetchOutcome::NotModified);
    }

    let response = response
        .error_for_status()
        .with_context(|| format!("Registry index request failed for '{url}'"))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_INDEX_BYTES as u64)
    {
        bail!("Registry index exceeds the {MAX_INDEX_BYTES}-byte size limit");
    }

    let etag = response_header(&response, header::ETAG);
    let last_modified = response_header(&response, header::LAST_MODIFIED);
    let bytes = read_limited_body(response).await?;

    parse_index(&bytes).context("Downloaded registry index is invalid")?;
    write_atomic(cache_file, &bytes).with_context(|| {
        format!(
            "Failed to cache registry index at '{}'",
            cache_file.display()
        )
    })?;
    write_cache_metadata(
        metadata_file,
        &RegistryCacheMetadata {
            source_url: url.to_string(),
            fetched_at: Utc::now().to_rfc3339(),
            etag,
            last_modified,
        },
    )?;
    Ok(FetchOutcome::Updated)
}

async fn read_limited_body(response: reqwest::Response) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read registry index response")?;
        if bytes.len().saturating_add(chunk.len()) > MAX_INDEX_BYTES {
            bail!("Registry index exceeds the {MAX_INDEX_BYTES}-byte size limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn response_header(response: &reqwest::Response, name: header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn usable_cache_metadata(
    cache_file: &Path,
    metadata_file: &Path,
    source_url: &str,
) -> Option<RegistryCacheMetadata> {
    load_index(cache_file).ok()?;
    let metadata: RegistryCacheMetadata =
        serde_json::from_slice(&fs::read(metadata_file).ok()?).ok()?;
    (metadata.source_url == source_url).then_some(metadata)
}

fn load_cached_index(
    cache_file: &Path,
    metadata_file: &Path,
    source_url: &str,
) -> Result<RegistryIndex> {
    if !cache_file.is_file() || !metadata_file.is_file() {
        bail!("No cached registry index is available. Run 'upstream add <NAME> --fetch' first.");
    }
    let metadata: RegistryCacheMetadata = serde_json::from_slice(
        &fs::read(metadata_file)
            .with_context(|| format!("Failed to read '{}'.", metadata_file.display()))?,
    )
    .with_context(|| {
        format!(
            "Invalid registry cache metadata '{}'.",
            metadata_file.display()
        )
    })?;
    if metadata.source_url != source_url {
        bail!(
            "The cached registry index came from '{}', but registry.index_url is '{}'. Run 'upstream add <NAME> --fetch' to refresh it.",
            metadata.source_url,
            source_url
        );
    }
    load_index(cache_file)
}

fn write_cache_metadata(path: &Path, metadata: &RegistryCacheMetadata) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(metadata).context("Failed to encode registry metadata")?;
    write_atomic(path, &bytes)
        .with_context(|| format!("Failed to cache registry metadata at '{}'", path.display()))
}

fn load_index(path: &Path) -> Result<RegistryIndex> {
    let bytes = fs::read(path)
        .with_context(|| format!("Failed to read registry index '{}'.", path.display()))?;
    parse_index(&bytes).with_context(|| format!("Invalid registry index '{}'.", path.display()))
}

fn parse_index(bytes: &[u8]) -> Result<RegistryIndex> {
    let index: RegistryIndex =
        serde_json::from_slice(bytes).context("Failed to parse registry index JSON")?;
    if index.version != SUPPORTED_INDEX_VERSION {
        bail!(
            "Unsupported registry index version {}; this build supports version {}",
            index.version,
            SUPPORTED_INDEX_VERSION
        );
    }
    if let Some((name, _)) = index
        .packages
        .iter()
        .find(|(_, package)| package.revision == 0)
    {
        bail!("Registry package '{name}' has invalid revision 0");
    }
    Ok(index)
}

fn joined_patterns(patterns: &[String]) -> Option<String> {
    (!patterns.is_empty()).then(|| patterns.join(","))
}

impl RegistryProvider {
    fn provider(&self) -> Provider {
        match self {
            Self::Github => Provider::Github,
            Self::Gitlab => Provider::Gitlab,
            Self::Gitea => Provider::Gitea,
        }
    }
}

impl RegistryTrust {
    fn trust_mode(&self) -> TrustMode {
        match self {
            Self::None => TrustMode::None,
            Self::BestEffort => TrustMode::BestEffort,
            Self::Checksum => TrustMode::Checksum,
            Self::Signature => TrustMode::Signature,
            Self::All => TrustMode::All,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        RegistryCacheMetadata, joined_patterns, load_cached_index, parse_index,
        write_cache_metadata,
    };
    use crate::utils::test_support;

    const EMPTY_INDEX: &[u8] = br#"{"version":1,"packages":{}}"#;

    #[test]
    fn parses_supported_index_and_package_metadata() {
        let index = parse_index(
            br#"{"version":1,"packages":{"tool":{"revision":2,"repo":"https://github.com/o/tool","provider":"github","desktop":false,"trust":"best-effort","match":["linux","x86_64"]}}}"#,
        )
        .expect("valid index");
        let package = index.packages.get("tool").expect("tool package");
        assert_eq!(package.revision, 2);
        assert_eq!(
            joined_patterns(&package.r#match).as_deref(),
            Some("linux,x86_64")
        );
        assert_eq!(joined_patterns(&package.exclude), None);
    }

    #[test]
    fn rejects_unsupported_index_version() {
        let error = parse_index(br#"{"version":2,"packages":{}}"#)
            .expect_err("unsupported version should fail");
        assert!(
            error
                .to_string()
                .contains("Unsupported registry index version 2")
        );
    }

    #[test]
    fn rejects_zero_package_revision() {
        let error = parse_index(
            br#"{"version":1,"packages":{"tool":{"revision":0,"repo":"https://github.com/o/tool","provider":"github","desktop":false,"trust":"checksum"}}}"#,
        )
        .expect_err("zero revision should fail");
        assert!(error.to_string().contains("invalid revision 0"));
    }

    #[test]
    fn cached_index_requires_metadata_and_matching_source() {
        let root = test_support::temp_root("registry-add", "cache-source");
        let cache_file = root.join("index.min.json");
        let metadata_file = root.join("metadata.json");
        fs::create_dir_all(&root).expect("cache directory");
        fs::write(&cache_file, EMPTY_INDEX).expect("index cache");

        let missing = load_cached_index(&cache_file, &metadata_file, "https://one.example/index")
            .expect_err("metadata is required");
        assert!(missing.to_string().contains("--fetch"));

        write_cache_metadata(
            &metadata_file,
            &RegistryCacheMetadata {
                source_url: "https://one.example/index".to_string(),
                fetched_at: "2026-07-22T00:00:00Z".to_string(),
                etag: Some("etag".to_string()),
                last_modified: None,
            },
        )
        .expect("cache metadata");

        load_cached_index(&cache_file, &metadata_file, "https://one.example/index")
            .expect("matching cache");
        let mismatch = load_cached_index(&cache_file, &metadata_file, "https://two.example/index")
            .expect_err("different source requires refresh");
        assert!(mismatch.to_string().contains("--fetch"));
        fs::remove_dir_all(root).expect("cleanup");
    }
}
