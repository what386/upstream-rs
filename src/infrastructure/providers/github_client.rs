use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GithubClientError {
    #[error("GitHub API request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Failed to write file: {0}")]
    FileWriteFailed(#[from] std::io::Error),

    #[error("Download size mismatch: expected {expected} bytes, got {actual} bytes")]
    SizeMismatch { expected: u64, actual: u64 },

    #[error("JSON deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, GithubClientError>;

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
    /// Creates a new GitHub client with optional authentication token.
    ///
    /// # Arguments
    /// * `token` - Optional GitHub personal access token for API authentication
    pub fn new(token: Option<&str>) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
        );

        if let Some(token) = token {
            let auth_value = format!("Bearer {}", token);
            headers.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&auth_value)
                    .map_err(|e| GithubClientError::RequestFailed(
                        reqwest::Error::from(e)
                    ))?,
            );
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self { client })
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let response = self.client.get(url).send().await?;
        response.error_for_status_ref()?;
        let data = response.json::<T>().await?;
        Ok(data)
    }

    /// Downloads a file from a URL to the specified destination path.
    ///
    /// # Arguments
    /// * `url` - The URL to download from
    /// * `destination_path` - The local path where the file should be saved
    /// * `progress_callback` - Optional callback for download progress updates (downloaded_bytes, total_bytes)
    pub async fn download_file<P, F>(
        &self,
        url: &str,
        destination_path: P,
        mut progress_callback: Option<F>,
    ) -> Result<()>
    where
        P: AsRef<Path>,
        F: FnMut(u64, u64),
    {
        let response = self.client.get(url).send().await?;
        response.error_for_status_ref()?;

        let total_bytes = response.content_length().unwrap_or(0);

        let mut file = File::create(destination_path).await?;
        let mut stream = response.bytes_stream();
        let mut total_read: u64 = 0;

        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            total_read += chunk.len() as u64;

            if let Some(ref mut callback) = progress_callback {
                callback(total_read, total_bytes);
            }
        }

        file.flush().await?;

        // Verify file size matches expected
        if total_bytes > 0 && total_read != total_bytes {
            return Err(GithubClientError::SizeMismatch {
                expected: total_bytes,
                actual: total_read,
            });
        }

        Ok(())
    }

    /// Gets the release for the specified repository matching the given tag name.
    ///
    /// # Arguments
    /// * `owner_repo` - The repository in the format "owner/repo"
    /// * `tag` - The tag name to retrieve (e.g., "v1.0.0")
    pub async fn get_release_by_tag(
        &self,
        owner_repo: &str,
        tag: &str,
    ) -> Result<GithubReleaseDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            owner_repo, tag
        );
        self.get_json(&url).await
    }

    /// Gets the release for the specified repository matching the given release ID.
    ///
    /// # Arguments
    /// * `owner_repo` - The repository in the format "owner/repo"
    /// * `release_id` - The unique identifier of the release
    pub async fn get_release_by_id(
        &self,
        owner_repo: &str,
        release_id: i64,
    ) -> Result<GithubReleaseDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/{}",
            owner_repo, release_id
        );
        self.get_json(&url).await
    }

    /// Gets the latest release for the specified repository.
    ///
    /// # Arguments
    /// * `owner_repo` - The repository in the format "owner/repo"
    pub async fn get_latest_release(&self, owner_repo: &str) -> Result<GithubReleaseDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            owner_repo
        );
        self.get_json(&url).await
    }

    /// Gets all releases for the specified repository, paginating through results as needed.
    ///
    /// # Arguments
    /// * `owner_repo` - The repository in the format "owner/repo"
    /// * `per_page` - The number of releases to fetch per API request (default is 30)
    pub async fn get_all_releases(
        &self,
        owner_repo: &str,
        per_page: u32,
    ) -> Result<Vec<GithubReleaseDto>> {
        let mut releases = Vec::new();
        let mut page = 1;

        loop {
            let url = format!(
                "https://api.github.com/repos/{}/releases?per_page={}&page={}",
                owner_repo, per_page, page
            );
            let batch: Vec<GithubReleaseDto> = self.get_json(&url).await?;

            if batch.is_empty() {
                break;
            }

            let batch_len = batch.len();
            releases.extend(batch);

            if batch_len < per_page as usize {
                break;
            }

            page += 1;
        }

        Ok(releases)
    }

    /// Gets the asset for the specified repository matching the given asset ID.
    ///
    /// # Arguments
    /// * `owner_repo` - The repository in the format "owner/repo"
    /// * `asset_id` - The unique identifier of the asset
    pub async fn get_asset_by_id(
        &self,
        owner_repo: &str,
        asset_id: i64,
    ) -> Result<GithubAssetDto> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/assets/{}",
            owner_repo, asset_id
        );
        self.get_json(&url).await
    }
}
