use anyhow::{Context, Result};
use reqwest::{Client, StatusCode, header};
use serde::Deserialize;
use std::path::Path;

use crate::{
    models::{provider::RepositorySearchFilters, upstream::DownloadConfig},
    providers::{download_handler, http::http_status},
};

use super::github_dtos::{
    GithubBranchDto, GithubReleaseDto, GithubRepositorySearchResponseDto, GithubTagDto,
};

#[derive(Debug, Clone)]
pub struct GithubClient {
    client: Client,
    download_config: DownloadConfig,
}

impl GithubClient {
    pub fn new(token: Option<&str>, download_config: DownloadConfig) -> Result<Self> {
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

        Ok(Self {
            client,
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

        http_status::error_for_status(&response, "GitHub API", url)?;

        let data = response
            .json::<T>()
            .await
            .context("Failed to parse JSON response")?;

        Ok(data)
    }

    async fn get_text_with_accept(&self, url: &str, accept: &'static str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .header(header::ACCEPT, accept)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        http_status::error_for_status(&response, "GitHub API", url)?;

        response
            .text()
            .await
            .context("Failed to read text response")
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
        let url = "https://api.github.com/user";
        self.client
            .get(url)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))
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

    pub async fn get_tag_by_name(&self, owner_repo: &str, tag: &str) -> Result<GithubTagDto> {
        let per_page = 100;
        let mut page = 1;

        loop {
            let tags = self
                .get_tags_page(owner_repo, per_page, page)
                .await
                .context(format!("Failed to get tags page {}", page))?;
            let partial_page = tags.len() < per_page as usize;

            if let Some(found) = tags.into_iter().find(|candidate| candidate.name == tag) {
                return Ok(found);
            }

            if partial_page {
                anyhow::bail!("Tag '{}' not found for {}", tag, owner_repo);
            }

            page += 1;
        }
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

    pub async fn get_latest_tag(&self, owner_repo: &str) -> Result<GithubTagDto> {
        let mut tags = self
            .get_tags_page(owner_repo, 1, 1)
            .await
            .context(format!("Failed to get latest tag for {}", owner_repo))?;

        tags.pop()
            .context(format!("No tags found for {}", owner_repo))
    }

    pub async fn get_tags_page(
        &self,
        owner_repo: &str,
        per_page: u32,
        page: u32,
    ) -> Result<Vec<GithubTagDto>> {
        let url = format!(
            "https://api.github.com/repos/{}/tags?per_page={}&page={}",
            owner_repo, per_page, page
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get tags page {}", page))
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
            let batch = self
                .get_releases_page(owner_repo, per_page, page)
                .await
                .context(format!("Failed to get releases page {}", page))?;
            let partial_page = batch.len() < per_page as usize;

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
    ) -> Result<Vec<GithubReleaseDto>> {
        let url = format!(
            "https://api.github.com/repos/{}/releases?per_page={}&page={}",
            owner_repo, per_page, page
        );
        self.get_json(&url)
            .await
            .context(format!("Failed to get releases page {}", page))
    }

    pub async fn get_branch_head_sha(&self, owner_repo: &str, branch: &str) -> Result<String> {
        let encoded_branch = branch.replace('/', "%2F");
        let url = format!(
            "https://api.github.com/repos/{}/branches/{}",
            owner_repo, encoded_branch
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context(format!("Failed to send request to {}", url))?;

        let status = response.status();
        if !status.is_success() {
            if let Some(message) =
                http_status::rate_limit_message(status, response.headers(), "GitHub API", &url)
            {
                anyhow::bail!("{message}");
            }
            if matches!(
                status,
                StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY
            ) {
                anyhow::bail!(
                    "Branch '{}' not found for {}; verify the branch name or omit --branch to build a release tag",
                    branch,
                    owner_repo
                );
            }
            http_status::error_for_status(&response, "GitHub API", &url)?;
        }

        let dto: GithubBranchDto = response
            .json()
            .await
            .context("Failed to parse JSON response")
            .context(format!(
                "Failed to get branch head for {}/{}",
                owner_repo, branch
            ))?;
        Ok(dto.commit.sha)
    }

    pub async fn get_project_readme(&self, owner_repo: &str) -> Result<String> {
        let url = format!("https://api.github.com/repos/{}/readme", owner_repo);
        self.get_text_with_accept(&url, "application/vnd.github.raw")
            .await
            .context(format!("Failed to get README for {}", owner_repo))
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
        if let Some(max_stars) = filters.max_stars {
            parts.push(format!("stars:<={max_stars}"));
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
        let json =
            include_str!("../../../tests/fixtures/providers/github-release-nullable-fields.json");

        let parsed = serde_json::from_str::<GithubReleaseDto>(json).expect("valid release JSON");
        assert_eq!(parsed.name, "");
        assert_eq!(parsed.body, "");
        assert_eq!(parsed.published_at, "");
        assert_eq!(parsed.assets[0].content_type, "");
        assert_eq!(parsed.assets[0].created_at, "");
    }

    #[test]
    fn github_search_dto_accepts_nullable_string_fields() {
        let json =
            include_str!("../../../tests/fixtures/providers/github-search-nullable-fields.json");

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
            Some(50_000),
            Some(NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()),
            true,
            false,
        );

        assert_eq!(
            GithubClient::build_repository_search_query("fast search", &filters),
            "fast search language:Rust topic:cli stars:>=100 stars:<=50000 pushed:>=2026-01-02 fork:true archived:false"
        );
    }

    #[test]
    fn build_repository_search_query_quotes_multi_word_qualifier_values() {
        let filters = RepositorySearchFilters::new(
            Some("Common Lisp".to_string()),
            None,
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
        let filters = RepositorySearchFilters::new(None, None, None, None, None, false, true);

        assert_eq!(
            GithubClient::build_repository_search_query("", &filters),
            "stars:>=0"
        );
    }
}
