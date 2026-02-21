use anyhow::{Context, Result, bail};
use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabLinkDto {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub direct_asset_url: Option<String>,
    pub link_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabSourceDto {
    pub format: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabAssetsDto {
    pub count: i64,
    pub sources: Vec<GitlabSourceDto>,
    pub links: Vec<GitlabLinkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitlabReleaseDto {
    pub tag_name: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub released_at: Option<String>,
    pub upcoming_release: Option<bool>,
    pub assets: GitlabAssetsDto,
}

#[derive(Debug, Clone)]
pub struct GitlabClient {
    client: Client,
    base_url: String,
}

impl GitlabClient {
    pub fn new(token: Option<&str>, base_url: Option<&str>) -> Result<Self> {
        let mut base = base_url.unwrap_or("https://gitlab.com").to_string();

        if !base.starts_with("http://") && !base.starts_with("https://") {
            base = format!("https://{}", base);
        }

        let mut headers = header::HeaderMap::new();
        let user_agent = format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_str(&user_agent)
                .context("Failed to create user agent header")?,
        );

        if let Some(token) = token {
            headers.insert(
                "PRIVATE-TOKEN",
                header::HeaderValue::from_str(token)
                    .context("Failed to create private token header")?,
            );
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url: base,
        })
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
            .context(format!("GitLab API returned error for {}", url))?;

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

    fn encode_project_path(project_path: &str) -> String {
        project_path.replace('/', "%2F")
    }

    pub async fn get_release_by_tag(
        &self,
        project_path: &str,
        tag: &str,
    ) -> Result<GitlabReleaseDto> {
        let encoded_path = Self::encode_project_path(project_path);
        let url = format!(
            "{}/api/v4/projects/{}/releases/{}",
            self.base_url, encoded_path, tag
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get release for tag {}", tag))
    }

    pub async fn get_releases(
        &self,
        project_path: &str,
        per_page: Option<u32>,
        max_total: Option<u32>,
    ) -> Result<Vec<GitlabReleaseDto>> {
        let per_page = per_page.unwrap_or(20).min(100);
        let encoded_path = Self::encode_project_path(project_path);
        let mut page = 1;
        let mut releases = Vec::new();

        loop {
            let url = format!(
                "{}/api/v4/projects/{}/releases?per_page={}&page={}",
                self.base_url, encoded_path, per_page, page
            );
            let batch: Vec<GitlabReleaseDto> = self
                .get_json(&url)
                .await
                .context(format!("Failed to get releases page {}", page))?;

            if batch.is_empty() {
                break;
            }

            releases.extend(batch);

            if let Some(max) = max_total
                && releases.len() >= max as usize
            {
                releases.truncate(max as usize);
                break;
            }

            if releases.len() % per_page as usize != 0 {
                break;
            }

            page += 1;
        }

        Ok(releases)
    }
}

#[cfg(test)]
#[path = "../../../tests/providers/gitlab/gitlab_client.rs"]
mod tests;
