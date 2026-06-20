use anyhow::{Context, Result};
use reqwest::{Client, header};
use serde::Deserialize;
use std::path::Path;

use crate::{models::upstream::DownloadConfig, providers::download_handler};

use super::gitea_dtos::GiteaReleaseDto;
#[derive(Debug, Deserialize)]
struct GiteaCommitRefDto {
    #[serde(default)]
    id: String,
    #[serde(default)]
    sha: String,
}

#[derive(Debug, Deserialize)]
struct GiteaBranchDto {
    commit: GiteaCommitRefDto,
}

#[derive(Debug, Clone)]
pub struct GiteaClient {
    client: Client,
    base_url: String,
    download_config: DownloadConfig,
}

impl GiteaClient {
    pub fn new(
        token: Option<&str>,
        base_url: Option<&str>,
        download_config: DownloadConfig,
    ) -> Result<Self> {
        let mut base = base_url.unwrap_or("https://gitea.com").to_string();

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
            let auth_value = format!("token {}", token);
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

        Ok(Self {
            client,
            base_url: base,
            download_config,
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
            .context(format!("Gitea API returned error for {}", url))?;

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
        download_handler::download_file(
            &self.client,
            url,
            destination,
            progress,
            self.download_config,
        )
        .await
    }

    pub async fn get_release_by_tag(&self, owner_repo: &str, tag: &str) -> Result<GiteaReleaseDto> {
        let url = format!(
            "{}/api/v1/repos/{}/releases/tags/{}",
            self.base_url, owner_repo, tag
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get release for tag {}", tag))
    }

    pub async fn get_latest_release(&self, owner_repo: &str) -> Result<GiteaReleaseDto> {
        let url = format!(
            "{}/api/v1/repos/{}/releases/latest",
            self.base_url, owner_repo
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
    ) -> Result<Vec<GiteaReleaseDto>> {
        let per_page = per_page.unwrap_or(30).min(50);
        let mut page = 1;
        let mut releases = Vec::new();

        loop {
            let batch = self
                .get_releases_page(owner_repo, per_page, page)
                .await
                .context(format!("Failed to get releases page {}", page))?;
            let partial_page = batch.len() < per_page as usize;

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

            if partial_page {
                break;
            }

            page += 1;
        }

        Ok(releases)
    }

    pub async fn get_releases_page(
        &self,
        owner_repo: &str,
        per_page: u32,
        page: u32,
    ) -> Result<Vec<GiteaReleaseDto>> {
        let url = format!(
            "{}/api/v1/repos/{}/releases?page={}&limit={}",
            self.base_url, owner_repo, page, per_page
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get releases page {}", page))
    }

    pub async fn get_branch_head_sha(&self, owner_repo: &str, branch: &str) -> Result<String> {
        let encoded_branch = branch.replace('/', "%2F");
        let url = format!(
            "{}/api/v1/repos/{}/branches/{}",
            self.base_url, owner_repo, encoded_branch
        );
        let dto: GiteaBranchDto = self.get_json(&url).await.context(format!(
            "Failed to get branch head for {}/{}",
            owner_repo, branch
        ))?;
        if !dto.commit.id.is_empty() {
            return Ok(dto.commit.id);
        }
        Ok(dto.commit.sha)
    }
}

#[cfg(test)]
mod tests {
    use super::GiteaClient;
    use crate::providers::gitea::gitea_dtos::GiteaReleaseDto;

    #[test]
    fn new_normalizes_base_url_without_scheme() {
        let client =
            GiteaClient::new(None, Some("gitea.example.com"), Default::default()).expect("client");
        assert_eq!(client.base_url, "https://gitea.example.com");
    }

    #[test]
    fn nullable_string_fields_deserialize_to_empty_strings() {
        let json = r#"
            {
              "id": 7,
              "tag_name": null,
              "name": null,
              "body": null,
              "prerelease": false,
              "draft": false,
              "published_at": null,
              "assets": [
                {
                  "id": 1,
                  "name": null,
                  "browser_download_url": null,
                  "size": 0,
                  "content_type": null,
                  "created_at": null
                }
              ]
            }
            "#;

        let parsed = serde_json::from_str::<GiteaReleaseDto>(json).expect("parse release");
        assert_eq!(parsed.tag_name, "");
        assert_eq!(parsed.name, "");
        assert_eq!(parsed.body, "");
        assert_eq!(parsed.published_at, "");
        assert_eq!(parsed.assets[0].name, "");
        assert_eq!(parsed.assets[0].browser_download_url, "");
    }
}
