use anyhow::{Context, Result};
use reqwest::{Client, header};
use serde::Deserialize;
use std::path::Path;

use crate::providers::download_handler;

use super::github_dtos::{GithubReleaseDto, GithubRepositorySearchResponseDto};
#[derive(Debug, Deserialize)]
struct GithubCommitDto {
    sha: String,
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
        download_handler::download_file(&self.client, url, destination, progress).await
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
                && releases.len() >= max as usize
            {
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

    pub async fn get_branch_head_sha(&self, owner_repo: &str, branch: &str) -> Result<String> {
        let encoded_branch = branch.replace('/', "%2F");
        let url = format!(
            "https://api.github.com/repos/{}/commits/{}",
            owner_repo, encoded_branch
        );
        let dto: GithubCommitDto = self.get_json(&url).await.context(format!(
            "Failed to get branch head for {}/{}",
            owner_repo, branch
        ))?;
        Ok(dto.sha)
    }

    pub async fn search_repositories(
        &self,
        query: &str,
        limit: Option<u32>,
    ) -> Result<GithubRepositorySearchResponseDto> {
        let per_page = limit.unwrap_or(10).clamp(1, 100);
        let mut url = reqwest::Url::parse("https://api.github.com/search/repositories")
            .context("Failed to build GitHub search URL")?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("per_page", &per_page.to_string());

        self.get_json(url.as_str())
            .await
            .context(format!("Failed to search repositories for '{}'", query))
    }
}

#[cfg(test)]
mod tests {
    use crate::providers::github::github_dtos::{
        GithubReleaseDto, GithubRepositorySearchResponseDto,
    };

    #[test]
    fn github_release_dto_accepts_nullable_string_fields() {
        let json = r#"
        {
          "id": 1,
          "tag_name": "v1.0.0",
          "name": null,
          "body": null,
          "prerelease": false,
          "draft": false,
          "published_at": null,
          "assets": [
            {
              "id": 42,
              "name": "tree-sitter-linux.tar.gz",
              "browser_download_url": "https://example.com/asset.tar.gz",
              "size": 1234,
              "content_type": null,
              "created_at": null
            }
          ]
        }
        "#;

        let parsed = serde_json::from_str::<GithubReleaseDto>(json).expect("valid release JSON");
        assert_eq!(parsed.name, "");
        assert_eq!(parsed.body, "");
        assert_eq!(parsed.published_at, "");
        assert_eq!(parsed.assets[0].content_type, "");
        assert_eq!(parsed.assets[0].created_at, "");
    }

    #[test]
    fn github_search_dto_accepts_nullable_string_fields() {
        let json = r#"
        {
          "items": [
            {
              "full_name": "BurntSushi/ripgrep",
              "name": "ripgrep",
              "description": null,
              "stargazers_count": 10,
              "language": null,
              "updated_at": null,
              "archived": false,
              "fork": false
            }
          ]
        }
        "#;

        let parsed = serde_json::from_str::<GithubRepositorySearchResponseDto>(json)
            .expect("valid search JSON");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].description, "");
        assert_eq!(parsed.items[0].language, "");
        assert_eq!(parsed.items[0].updated_at, "");
    }
}
