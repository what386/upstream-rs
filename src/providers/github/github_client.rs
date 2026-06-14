use anyhow::{Context, Result};
use reqwest::{Client, header};
use serde::Deserialize;
use std::path::Path;

use crate::{models::provider::RepositorySearchFilters, providers::download_handler};

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
        filters: &RepositorySearchFilters,
    ) -> Result<GithubRepositorySearchResponseDto> {
        let per_page = limit.unwrap_or(10).clamp(1, 100);
        let search_query = Self::build_repository_search_query(query, filters);
        let mut url = reqwest::Url::parse("https://api.github.com/search/repositories")
            .context("Failed to build GitHub search URL")?;
        url.query_pairs_mut()
            .append_pair("q", &search_query)
            .append_pair("per_page", &per_page.to_string());

        self.get_json(url.as_str()).await.context(format!(
            "Failed to search repositories for '{}'",
            search_query
        ))
    }

    fn build_repository_search_query(query: &str, filters: &RepositorySearchFilters) -> String {
        let mut parts = vec![query.trim().to_string()];

        if let Some(language) = &filters.language {
            parts.push(format!(
                "language:{}",
                Self::format_search_qualifier_value(language)
            ));
        }
        if let Some(topic) = &filters.topic {
            parts.push(format!(
                "topic:{}",
                Self::format_search_qualifier_value(topic)
            ));
        }
        if let Some(min_stars) = filters.min_stars {
            parts.push(format!("stars:>={min_stars}"));
        }
        if let Some(pushed_after) = filters.pushed_after {
            parts.push(format!("pushed:>={pushed_after}"));
        }
        if filters.include_forks {
            parts.push("fork:true".to_string());
        }
        if !filters.include_archived {
            parts.push("archived:false".to_string());
        }

        let parts = parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return "stars:>=0".to_string();
        }

        parts.join(" ")
    }

    fn format_search_qualifier_value(value: &str) -> String {
        if value.chars().any(char::is_whitespace) {
            format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
        } else {
            value.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use crate::models::provider::RepositorySearchFilters;
    use crate::providers::github::GithubClient;
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

    #[test]
    fn build_repository_search_query_adds_discovery_filters() {
        let filters = RepositorySearchFilters::new(
            Some("Rust".to_string()),
            Some("cli".to_string()),
            Some(100),
            Some(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()),
            true,
            false,
        );

        assert_eq!(
            GithubClient::build_repository_search_query("fast search", &filters),
            "fast search language:Rust topic:cli stars:>=100 pushed:>=2026-01-02 fork:true archived:false"
        );
    }

    #[test]
    fn build_repository_search_query_quotes_multi_word_qualifier_values() {
        let filters = RepositorySearchFilters::new(
            Some("Common Lisp".to_string()),
            None,
            None,
            None,
            false,
            false,
        );

        assert_eq!(
            GithubClient::build_repository_search_query("editor", &filters),
            "editor language:\"Common Lisp\" archived:false"
        );
    }

    #[test]
    fn build_repository_search_query_falls_back_when_query_and_filters_are_empty() {
        let filters = RepositorySearchFilters::new(None, None, None, None, false, true);

        assert_eq!(
            GithubClient::build_repository_search_query("", &filters),
            "stars:>=0"
        );
    }
}
