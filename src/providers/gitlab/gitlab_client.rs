use anyhow::{Context, Result};
use reqwest::{Client, header};
use serde::Deserialize;
use std::path::Path;

use crate::{
    models::upstream::DownloadConfig,
    providers::{download_handler, http::http_status},
};

use super::gitlab_dtos::GitlabReleaseDto;
#[derive(Debug, Deserialize)]
struct GitlabCommitRefDto {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GitlabBranchDto {
    commit: GitlabCommitRefDto,
}

#[derive(Debug, Clone)]
pub struct GitlabClient {
    client: Client,
    base_url: String,
    download_config: DownloadConfig,
}

impl GitlabClient {
    pub fn new(
        token: Option<&str>,
        base_url: Option<&str>,
        download_config: DownloadConfig,
    ) -> Result<Self> {
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

        http_status::error_for_status(&response, "GitLab API", url)?;

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

    pub async fn check_token(&self) -> Result<reqwest::Response> {
        let url = format!("{}/api/v4/user", self.base_url);
        self.client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))
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
        let mut page = 1;
        let mut releases = Vec::new();

        loop {
            let batch = self
                .get_releases_page(project_path, per_page, page)
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
        project_path: &str,
        per_page: u32,
        page: u32,
    ) -> Result<Vec<GitlabReleaseDto>> {
        let encoded_path = Self::encode_project_path(project_path);
        let url = format!(
            "{}/api/v4/projects/{}/releases?per_page={}&page={}",
            self.base_url, encoded_path, per_page, page
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get releases page {}", page))
    }

    pub async fn get_branch_head_sha(&self, project_path: &str, branch: &str) -> Result<String> {
        let encoded_path = Self::encode_project_path(project_path);
        let encoded_branch = Self::encode_project_path(branch);
        let url = format!(
            "{}/api/v4/projects/{}/repository/branches/{}",
            self.base_url, encoded_path, encoded_branch
        );
        let dto: GitlabBranchDto = self.get_json(&url).await.context(format!(
            "Failed to get branch head for {}/{}",
            project_path, branch
        ))?;
        Ok(dto.commit.id)
    }
}

#[cfg(test)]
mod tests {
    use super::GitlabClient;
    use crate::providers::gitlab::gitlab_dtos::GitlabReleaseDto;

    #[test]
    fn new_normalizes_base_url_without_scheme() {
        let client = GitlabClient::new(None, Some("gitlab.example.com"), Default::default())
            .expect("client");
        assert_eq!(client.base_url, "https://gitlab.example.com");
    }

    #[test]
    fn encode_project_path_percent_encodes_slashes() {
        assert_eq!(
            GitlabClient::encode_project_path("group/subgroup/project"),
            "group%2Fsubgroup%2Fproject"
        );
    }

    #[test]
    fn gitlab_release_dto_deserializes_minimal_valid_payload() {
        let json = r#"
            {
              "tag_name": "v1.0.0",
              "name": "v1.0.0",
              "description": "notes",
              "created_at": "2026-02-21T00:00:00Z",
              "released_at": null,
              "upcoming_release": false,
              "assets": { "count": 0, "sources": [], "links": [] }
            }
            "#;

        let parsed = serde_json::from_str::<GitlabReleaseDto>(json).expect("parse release");
        assert_eq!(parsed.tag_name, "v1.0.0");
        assert_eq!(parsed.assets.count, 0);
        assert!(parsed.assets.links.is_empty());
    }
}
