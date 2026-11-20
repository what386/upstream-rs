use crate::{
    application::cancellation, models::upstream::config::DownloadConfig,
    providers::shared::download_handler,
};
use anyhow::{Context, Result, bail};
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{sync::Mutex, time::Instant};

static NEXT_API_REQUEST: OnceLock<Mutex<Instant>> = OnceLock::new();

#[derive(Clone)]
pub struct CratesIoClient {
    http: Client,
    cache: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    download_config: DownloadConfig,
}

pub(super) fn http_client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(format!(
            "upstream-rs/{} (https://github.com/what386/upstream-rs)",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(60))
        .build()?)
}

impl CratesIoClient {
    pub fn new(download_config: DownloadConfig) -> Result<Self> {
        Ok(Self {
            http: http_client()?,
            cache: Arc::new(Mutex::new(HashMap::new())),
            download_config,
        })
    }

    pub(super) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        cancellation::check()?;
        let mut url = Url::parse("https://crates.io/api/v1/")?.join(path)?;
        url.query_pairs_mut().extend_pairs(query.iter().copied());
        let mut cache = tokio::select! { cache = self.cache.lock() => cache, _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()) };
        if let Some(value) = cache.get(url.as_str()) {
            return Ok(serde_json::from_value(value.clone())?);
        }

        tracing::debug!(%url, "Fetching crates.io API");
        for attempt in 0..3 {
            let mut next = tokio::select! {
                next = NEXT_API_REQUEST.get_or_init(|| Mutex::new(Instant::now())).lock() => next,
                _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()),
            };

            wait_until(*next).await?;
            *next = Instant::now() + Duration::from_secs(1);
            let response = tokio::select! { r = self.http.get(url.clone()).send() => r, _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()) };
            match response {
                Ok(response) => {
                    if (response.status() == StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error())
                        && attempt < 2
                    {
                        let delay = response
                            .headers()
                            .get(reqwest::header::RETRY_AFTER)
                            .and_then(|h| h.to_str().ok())
                            .and_then(retry_after)
                            .unwrap_or(Duration::from_secs(1 << attempt));

                        *next = (*next).max(Instant::now() + delay);
                        drop(next);
                        continue;
                    }

                    drop(next);
                    let response = response
                        .error_for_status()
                        .with_context(|| format!("crates.io API request failed: {url}"))?;

                    let value: serde_json::Value = tokio::select! { v = response.json() => v?, _ = cancellation::cancelled() => return Err(cancellation::Cancelled.into()) };
                    let result = serde_json::from_value(value.clone())
                        .context("Invalid crates.io API response")?;

                    cache.insert(url.to_string(), value);
                    return Ok(result);
                }
                Err(error) if attempt == 2 => return Err(error.into()),
                Err(_) => drop(next),
            }
        }

        bail!("crates.io API request failed")
    }

    pub(super) async fn download(
        &self,
        url: &str,
        destination: &Path,
        progress: &mut Option<&mut (dyn FnMut(u64, u64) + '_)>,
    ) -> Result<()> {
        download_handler::download_file(
            &self.http,
            url,
            destination,
            progress,
            self.download_config,
        )
        .await
    }
}

pub(super) fn retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.parse() {
        return Some(Duration::from_secs(seconds));
    }

    let date = jiff::fmt::rfc2822::DateTimeParser::new()
        .parse_timestamp(value)
        .ok()?;

    Some(Duration::from_secs(
        (date.as_second() - jiff::Timestamp::now().as_second()).max(0) as u64,
    ))
}

pub(super) async fn wait_until(deadline: Instant) -> Result<()> {
    tokio::select! { _ = tokio::time::sleep_until(deadline) => Ok(()), _ = cancellation::cancelled() => Err(cancellation::Cancelled.into()) }
}
