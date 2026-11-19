use super::{
    crate_name,
    cratesio_client::{http_client, retry_after, wait_until},
    cratesio_dtos::IndexVersion,
};
use crate::{application::cancellation, utils::filesystem::atomic_ops::write_atomic};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, StatusCode, header};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::Mutex, time::Instant};

type CrateState = Arc<Mutex<Option<Arc<Vec<IndexVersion>>>>>;
#[derive(Clone)]
pub struct CratesIoIndexClient {
    http: Client,
    cache_dir: PathBuf,
    entries: Arc<Mutex<HashMap<String, CrateState>>>,
}
#[derive(Serialize, Deserialize)]
struct CachedIndex {
    etag: Option<String>,
    last_modified: Option<String>,
    body: String,
}
impl CachedIndex {
    fn conditional_request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(etag) = &self.etag {
            request.header(header::IF_NONE_MATCH, etag)
        } else if let Some(modified) = &self.last_modified {
            request.header(header::IF_MODIFIED_SINCE, modified)
        } else {
            request
        }
    }
}
impl CratesIoIndexClient {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            http: http_client()?,
            cache_dir,
            entries: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    pub(super) async fn versions(&self, name: &str) -> Result<Arc<Vec<IndexVersion>>> {
        cancellation::check()?;
        let name = crate_name(name)?.to_ascii_lowercase();
        // Different crates never wait on one another's HTTP requests.
        let state = self
            .entries
            .lock()
            .await
            .entry(name.clone())
            .or_default()
            .clone();
        let mut state = tokio::select! { s = state.lock() => s, _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()) };
        if let Some(versions) = state.as_ref() {
            return Ok(versions.clone());
        }
        let path = self.cache_dir.join(format!("{name}.json"));
        let cached = read_cache(&path, &name).await;
        let versions = Arc::new(self.refresh(&name, &path, cached).await?);
        *state = Some(versions.clone());
        Ok(versions)
    }
    async fn refresh(
        &self,
        name: &str,
        path: &Path,
        cached: Option<CachedIndex>,
    ) -> Result<Vec<IndexVersion>> {
        let url = format!("https://index.crates.io/{}", index_path(name)?);
        tracing::debug!(%url, "Refreshing crates.io sparse index");
        for attempt in 0..3 {
            let request = self.http.get(&url);
            let request = match &cached {
                Some(cache) => cache.conditional_request(request),
                None => request,
            };
            let response = tokio::select! { r = request.send() => r, _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()) };
            let response = match response {
                Ok(response) => response,
                Err(error) if attempt == 2 => {
                    return Err(error).context("Failed to fetch crates.io index");
                }
                Err(_) => {
                    wait_until(Instant::now() + Duration::from_secs(1 << attempt)).await?;
                    continue;
                }
            };
            if response.status() == StatusCode::NOT_MODIFIED {
                if let Some(cache) = &cached {
                    tracing::debug!(crate_name = name, "Sparse index unchanged (304)");
                    return parse_index(&cache.body, name);
                }
                ensure!(
                    attempt < 2,
                    "Index returned 304 without a valid cache for '{name}'"
                );
                continue;
            }
            if (response.status() == StatusCode::TOO_MANY_REQUESTS
                || response.status().is_server_error())
                && attempt < 2
            {
                let delay = response
                    .headers()
                    .get(header::RETRY_AFTER)
                    .and_then(|h| h.to_str().ok())
                    .and_then(retry_after)
                    .unwrap_or(Duration::from_secs(1 << attempt));
                wait_until(Instant::now() + delay).await?;
                continue;
            }
            let response = response
                .error_for_status()
                .with_context(|| format!("crates.io index request failed: {url}"))?;
            let etag = response
                .headers()
                .get(header::ETAG)
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned);
            let last_modified = response
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned);
            let body = tokio::select! { b = response.text() => b?, _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()) };
            let versions = parse_index(&body, name)?;
            let bytes = serde_json::to_vec(&CachedIndex {
                etag,
                last_modified,
                body,
            })?;
            let path = path.to_owned();
            if let Err(error) = tokio::task::spawn_blocking(move || write_atomic(&path, &bytes))
                .await
                .context("Index cache task failed")
                .and_then(|r| r)
            {
                tracing::warn!(%error, "Could not save sparse index cache");
            }
            return Ok(versions);
        }
        bail!("Failed to refresh index for '{name}'")
    }
}
async fn read_cache(path: &Path, name: &str) -> Option<CachedIndex> {
    let cache: CachedIndex = serde_json::from_slice(&tokio::fs::read(path).await.ok()?).ok()?;
    if !cache
        .etag
        .iter()
        .chain(&cache.last_modified)
        .all(|v| header::HeaderValue::from_str(v).is_ok())
    {
        return None;
    }
    parse_index(&cache.body, name).ok()?;
    Some(cache)
}
fn index_path(name: &str) -> Result<String> {
    let name = crate_name(name)?.to_ascii_lowercase();
    Ok(match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    })
}
fn parse_index(body: &str, name: &str) -> Result<Vec<IndexVersion>> {
    #[derive(Deserialize)]
    struct Format {
        v: Option<u32>,
    }
    let mut versions = Vec::new();
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        let format: Format = serde_json::from_str(line).context("Invalid sparse-index entry")?;
        if !matches!(format.v.unwrap_or(1), 1 | 2) {
            continue;
        }
        let version: IndexVersion =
            serde_json::from_str(line).context("Invalid sparse-index entry")?;
        ensure!(
            version.name.eq_ignore_ascii_case(name),
            "Index returned a different crate"
        );
        version.release()?;
        versions.push(version);
    }
    ensure!(
        !versions.is_empty(),
        "No supported index versions for '{name}'"
    );
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_paths_follow_crate_name_lengths() {
        for (name, expected) in [
            ("a", "1/a"),
            ("ab", "2/ab"),
            ("AbC", "3/a/abc"),
            ("Serde_Json", "se/rd/serde_json"),
        ] {
            assert_eq!(index_path(name).unwrap(), expected);
        }
        for name in ["", "../serde", "é", "a/b"] {
            assert!(index_path(name).is_err());
        }
    }

    #[test]
    fn conditional_requests_prefer_etag_and_fall_back_to_modified_time() {
        let client = Client::new();
        let mut cache = CachedIndex {
            etag: Some("\"version\"".into()),
            last_modified: Some("Wed, 18 Nov 2026 00:00:00 GMT".into()),
            body: String::new(),
        };
        let request = cache
            .conditional_request(client.get("https://index.crates.io/se/rd/serde"))
            .build()
            .unwrap();
        assert_eq!(request.headers()[header::IF_NONE_MATCH], "\"version\"");
        assert!(!request.headers().contains_key(header::IF_MODIFIED_SINCE));
        cache.etag = None;
        let request = cache
            .conditional_request(client.get("https://index.crates.io/se/rd/serde"))
            .build()
            .unwrap();
        assert_eq!(
            request.headers()[header::IF_MODIFIED_SINCE],
            cache.last_modified.unwrap()
        );
    }

    #[tokio::test]
    async fn corrupt_cache_is_a_miss() {
        let root = crate::utils::test_support::temp_root("cratesio", "corrupt-cache");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let path = root.join("serde.json");
        for bytes in [
            b"not json".as_slice(),
            br#"{"etag":"invalid\nheader","last_modified":null,"body":""}"#,
        ] {
            tokio::fs::write(&path, bytes).await.unwrap();
            assert!(read_cache(&path, "serde").await.is_none());
        }
        assert!(parse_index("", "serde").is_err());
        assert!(parse_index("not JSON", "serde").is_err());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
