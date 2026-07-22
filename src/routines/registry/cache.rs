use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use futures_util::StreamExt;
use reqwest::{StatusCode, header};
use serde::{Deserialize, Serialize};

use crate::utils::filesystem::atomic_ops::write_atomic;

use super::schema::{RegistryIndex, parse_index};

const MAX_INDEX_BYTES: usize = 10 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize, Serialize)]
struct RegistryCacheMetadata {
    source_url: String,
    fetched_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_modified: Option<String>,
}

pub enum FetchOutcome {
    Updated,
    NotModified,
}

pub(super) async fn fetch_index(
    url: &str,
    cache_file: &Path,
    metadata_file: &Path,
) -> Result<FetchOutcome> {
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

pub(super) fn load_cached_index(
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

#[cfg(test)]
mod tests {
    use super::{RegistryCacheMetadata, load_cached_index, write_cache_metadata};
    use crate::utils::test_support;
    use std::fs;

    #[test]
    fn cached_index_requires_metadata_and_matching_source() {
        let root = test_support::temp_root("registry-add", "cache-source");
        let cache_file = root.join("index.min.json");
        let metadata_file = root.join("metadata.json");
        fs::create_dir_all(&root).expect("cache directory");
        fs::write(&cache_file, br#"{"version":1,"packages":{}}"#).expect("index cache");
        assert!(
            load_cached_index(&cache_file, &metadata_file, "https://one.example/index")
                .unwrap_err()
                .to_string()
                .contains("--fetch")
        );
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
        assert!(
            load_cached_index(&cache_file, &metadata_file, "https://two.example/index")
                .unwrap_err()
                .to_string()
                .contains("--fetch")
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
