use anyhow::{Context, Result, bail};
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubAssetDto {
    pub id: i64,
    pub name: String,
    pub browser_download_url: String,
    pub size: i64,
    pub content_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubReleaseDto {
    pub id: i64,
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub prerelease: bool,
    pub draft: bool,
    pub published_at: String,
    pub assets: Vec<GithubAssetDto>,
}

#[derive(Debug, Clone)]
pub struct GithubClient {
    client: Client,
}

impl GithubClient {
    pub fn new(token: Option<&str>) -> Result<Self> {
        let mut headers = header::HeaderMap::new();

        let user_agent = format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_str(&user_agent)
                .context("Failed to create user agent header")?,
        );

        if let Some(token) = token {
            let auth_value = format!("Bearer {}", token);
            headers.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&auth_value)
                    .context("Failed to create authorization header")?,
            );
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { client })
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        response
            .error_for_status_ref()
            .context(format!("GitHub API returned error for {}", url))?;

        let data = response
            .json::<T>()
            .await
            .context("Failed to parse JSON response")?;

        Ok(data)
    }

    pub async fn download_file<F>(
        &self,
        url: &str,
        destination: &Path,
        progress: &mut Option<F>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
    {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context(format!("Failed to download from {}", url))?;

        response
            .error_for_status_ref()
            .context("Download request failed")?;

        let total_bytes = response.content_length().unwrap_or(0);

        let mut file = File::create(destination)
            .await
            .context(format!("Failed to create file at {:?}", destination))?;

        let mut stream = response.bytes_stream();
        let mut total_read: u64 = 0;

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read download chunk")?;

            file.write_all(&chunk)
                .await
                .context("Failed to write to file")?;

            total_read += chunk.len() as u64;

            if let Some(cb) = progress.as_mut() {
                cb(total_read, total_bytes);
            }
        }

        file.flush().await.context("Failed to flush file")?;

        if total_bytes > 0 && total_read != total_bytes {
            bail!(
                "Download size mismatch: expected {} bytes, got {} bytes",
                total_bytes,
                total_read
            );
        }

        Ok(())
    }

    pub async fn get_release_by_tag(
        &self,
        owner_repo: &str,
        tag: &str,
    ) -> Result<GithubReleaseDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            owner_repo, tag
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get release for tag {}", tag))
    }

    pub async fn get_release_by_id(
        &self,
        owner_repo: &str,
        release_id: i64,
    ) -> Result<GithubReleaseDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/{}",
            owner_repo, release_id
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get release with ID {}", release_id))
    }

    pub async fn get_latest_release(&self, owner_repo: &str) -> Result<GithubReleaseDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            owner_repo
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get latest release for {}", owner_repo))
    }

    pub async fn get_releases(
        &self,
        owner_repo: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<GithubReleaseDto>> {
        let per_page = per_page.unwrap_or(30);
        let mut page = 1;
        let mut releases = Vec::new();

        loop {
            let url = format!(
                "https://api.github.com/repos/{}/releases?per_page={}&page={}",
                owner_repo, per_page, page
            );
            let batch: Vec<GithubReleaseDto> = self
                .get_json(&url)
                .await
                .context(format!("Failed to get releases page {}", page))?;

            if batch.is_empty() {
                break;
            }

            releases.extend(batch);

            // Check if we've hit the total limit
            if let Some(max) = max_total
                && releases.len() >= max as usize {
                    releases.truncate(max as usize);
                    break;
                }

            // Check if this was a partial page (last page)
            if releases.len() % per_page as usize != 0 {
                break;
            }

            page += 1;
        }

        Ok(releases)
    }

    pub async fn get_asset_by_id(&self, owner_repo: &str, asset_id: i64) -> Result<GithubAssetDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/assets/{}",
            owner_repo, asset_id
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get asset with ID {}", asset_id))
    }
}
