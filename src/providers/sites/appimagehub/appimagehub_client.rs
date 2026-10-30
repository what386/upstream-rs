use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use reqwest::Url;
use serde_json::Value;

use crate::{
    models::upstream::config::DownloadConfig,
    providers::sites::http::{ConditionalDocumentResult, HttpClient},
};

use super::appimagehub_dtos::AppImageHubContentDto;

const API_ROOT: &str = "https://api.opendesktop.org/ocs/v1/";

/// Accept a numeric listing ID, canonical reference, or a shared-store product URL.
pub fn content_id(source: &str) -> Result<String> {
    let source = source.trim();
    let value = source.strip_prefix("appimagehub:").unwrap_or(source);
    let id = if let Ok(url) = Url::parse(value) {
        ensure!(
            matches!(url.scheme(), "http" | "https"),
            "AppImageHub requires an HTTP(S) product URL"
        );

        let host = url.host_str().unwrap_or_default();
        let host = host.strip_prefix("www.").unwrap_or(host);
        ensure!(
            matches!(
                host,
                "appimagehub.com" | "opendesktop.org" | "pling.com" | "store.kde.org"
            ),
            "Not an AppImageHub/openDesktop product URL"
        );

        let parts = url.path().trim_matches('/').split('/').collect::<Vec<_>>();
        ensure!(
            parts.len() == 2 && parts[0] == "p",
            "Expected an AppImageHub URL ending in /p/<id>"
        );

        parts[1].to_owned()
    } else {
        value.to_owned()
    };

    ensure!(
        !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()),
        "AppImageHub requires a numeric content ID or https://www.appimagehub.com/p/<id>"
    );

    let id: u64 = id.parse().context("AppImageHub content ID is too large")?;
    ensure!(id > 0, "AppImageHub content ID must be positive");
    Ok(id.to_string())
}

#[derive(Debug, Clone)]
pub struct AppImageHubClient {
    client: HttpClient,
    api_root: Url,
}

impl AppImageHubClient {
    /// A base override is the OCS API root, including /ocs/v1/ when required.
    pub fn new(base_url: Option<&str>, download_config: DownloadConfig) -> Result<Self> {
        let base = base_url.unwrap_or(API_ROOT);
        let api_root = Url::parse(&format!("{}/", base.trim_end_matches('/')))
            .context("Invalid AppImageHub OCS API URL")?;

        ensure!(
            matches!(api_root.scheme(), "http" | "https"),
            "AppImageHub API requires HTTP(S)"
        );

        Ok(Self {
            client: HttpClient::new(download_config)?,
            api_root,
        })
    }

    pub async fn get_content(&self, slug: &str, archived: bool) -> Result<AppImageHubContentDto> {
        let id = content_id(slug)?;
        let suffix = if archived { "/archived" } else { "" };
        let mut url = self.api_root.join(&format!("content/data/{id}{suffix}"))?;
        url.query_pairs_mut().append_pair("format", "json");
        let ConditionalDocumentResult::Document(document) = self
            .client
            .fetch_document_if_modified_since(url.as_str(), None)
            .await?
        else {
            bail!("Unexpected not-modified response from AppImageHub")
        };

        let response: Value = serde_json::from_slice(&document.body)
            .context("AppImageHub OCS API did not return JSON (possibly an anti-bot challenge)")?;

        AppImageHubContentDto::from_response(&response, &id)
    }

    pub async fn download_file<F>(
        &self,
        url: &str,
        destination: &Path,
        callback: &mut Option<F>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
    {
        self.client.download_file(url, destination, callback).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{common::enums::Provider, upstream::Package},
        providers::discovery::{SourceKind, infer_source, normalize_source_for_provider},
    };

    #[test]
    fn normalizes_ids_references_and_product_urls() {
        for source in [
            "2259793",
            "appimagehub:2259793",
            "https://www.appimagehub.com/p/2259793/",
            "https://www.opendesktop.org/p/2259793?x=1",
        ] {
            assert_eq!(content_id(source).unwrap(), "2259793");
            assert_eq!(
                normalize_source_for_provider(source, &Provider::AppImageHub, None),
                "2259793"
            );
        }

        assert_eq!(content_id("00042").unwrap(), "42");
        for source in [
            "",
            "0",
            "-1",
            "1/2",
            "../1",
            "appimagehub:abc",
            "https://evil.test/p/2259793",
            "https://appimagehub.com/not-a-listing",
            "https://appimagehub.com/p/2259793/file",
            "18446744073709551616",
        ] {
            assert!(content_id(source).is_err(), "{source}");
        }

        for source in [
            "appimagehub:2259793",
            "https://www.appimagehub.com/p/2259793",
        ] {
            let source = infer_source(source).unwrap();
            assert_eq!(source.provider, Provider::AppImageHub);
            assert_eq!(source.repo_slug, "2259793");
            assert_eq!(source.kind, SourceKind::CatalogEntry);
            assert_eq!(
                Package::key(&source.provider, &source.repo_slug),
                "appimagehub:2259793"
            );
        }

        assert!(infer_source("https://appimagehub.com/p/not-numeric").is_err());
        assert_eq!(
            "appimagehub".parse::<Provider>().unwrap(),
            Provider::AppImageHub
        );

        assert_eq!(
            serde_json::from_str::<Provider>(
                &serde_json::to_string(&Provider::AppImageHub).unwrap()
            )
            .unwrap(),
            Provider::AppImageHub
        );
    }
}
