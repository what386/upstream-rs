use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use crate::{
    application::context::CommandContext,
    models::{
        common::enums::Provider,
        provider::{RepositorySearchFilters, RepositorySearchResult},
    },
    output,
    output::pager,
};
use std::fmt::Write as _;

pub struct SearchResults {
    pub query: String,
    pub provider: Provider,
    pub base_url: Option<String>,
    pub limit: u32,
    pub filters: RepositorySearchFilters,
    pub results: Vec<RepositorySearchResult>,
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    query_words: Vec<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    limit: u32,
    language: Option<String>,
    topic: Option<String>,
    min_stars: Option<u64>,
    max_stars: Option<u64>,
    pushed_after: Option<NaiveDate>,
    include_forks: bool,
    include_archived: bool,
    json: bool,
) -> Result<()> {
    let query = query_words.join(" ").trim().to_string();
    let filters = RepositorySearchFilters::new(
        language,
        topic,
        min_stars,
        max_stars,
        pushed_after,
        include_forks,
        include_archived,
    );

    let search = search_repositories(query, provider, base_url, limit, filters).await?;
    let query = search.query;
    let effective_provider = search.provider;
    let base_url = search.base_url;
    let limit = search.limit;
    let filters = search.filters;
    let results = search.results;

    if results.is_empty() {
        if json {
            let result = json_search_result(
                &query,
                &effective_provider,
                base_url.as_deref(),
                limit,
                &filters,
                &results,
            );
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        println!("{}", output::warning("No repositories found."));
        return Ok(());
    }

    if json {
        let result = json_search_result(
            &query,
            &effective_provider,
            base_url.as_deref(),
            limit,
            &filters,
            &results,
        );
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    let title = if query.is_empty() {
        format!("Search via {}", effective_provider)
    } else {
        format!("Search: '{}' via {}", query, effective_provider)
    };
    pager::page_text(Some(&title), &format_results(&results))?;
    Ok(())
}

pub async fn search_repositories(
    query: String,
    provider: Option<Provider>,
    base_url: Option<String>,
    limit: u32,
    filters: RepositorySearchFilters,
) -> Result<SearchResults> {
    let context = CommandContext::new()?;
    let effective_provider = provider.unwrap_or(Provider::Github);
    let effective_limit = limit.max(1);

    let results = context
        .provider_manager
        .search_repositories(
            &query,
            &effective_provider,
            Some(effective_limit),
            &filters,
            base_url.as_deref(),
        )
        .await?;

    Ok(SearchResults {
        query,
        provider: effective_provider,
        base_url,
        limit: effective_limit,
        filters,
        results,
    })
}

#[derive(Serialize)]
struct JsonSearchResult {
    query: String,
    provider: String,
    base_url: Option<String>,
    limit: u32,
    filters: RepositorySearchFilters,
    results: Vec<JsonRepositorySearchResult>,
}

#[derive(Serialize)]
struct JsonRepositorySearchResult {
    repo_slug: String,
    display_name: String,
    description: String,
    stars: u64,
    language: String,
    updated_at: String,
}

fn json_search_result(
    query: &str,
    provider: &Provider,
    base_url: Option<&str>,
    limit: u32,
    filters: &RepositorySearchFilters,
    results: &[RepositorySearchResult],
) -> JsonSearchResult {
    JsonSearchResult {
        query: query.to_string(),
        provider: provider.to_string(),
        base_url: base_url.map(str::to_string),
        limit,
        filters: filters.clone(),
        results: results
            .iter()
            .map(|result| JsonRepositorySearchResult {
                repo_slug: result.repo_slug.clone(),
                display_name: result.display_name.clone(),
                description: result.description.clone(),
                stars: result.stars,
                language: result.language.clone(),
                updated_at: result.updated_at.to_rfc3339(),
            })
            .collect(),
    }
}

fn format_results(results: &[RepositorySearchResult]) -> String {
    let widths = SearchColumnWidths::from_rows(results);
    let mut out = String::new();

    writeln!(
        out,
        "{:<slug$} {:>stars$} {:<lang$} {:<updated$} Description",
        "Slug",
        "Stars",
        "Lang",
        "Updated",
        slug = widths.slug,
        stars = widths.stars,
        lang = widths.lang,
        updated = widths.updated,
    )
    .expect("write search header");
    writeln!(out, "{}", output::divider(widths.table_width())).expect("write search divider");

    for row in results {
        write_row(&mut out, row, &widths);
    }

    writeln!(out).expect("write search spacer");
    writeln!(out, "{} results - use --limit to see more", results.len())
        .expect("write search footer");
    out
}

fn write_row(out: &mut String, row: &RepositorySearchResult, widths: &SearchColumnWidths) {
    writeln!(
        out,
        "{:<slug$} {:>stars$} {:<lang$} {:<updated$} {}",
        truncate(&row.repo_slug, widths.slug),
        format_stars(row.stars),
        truncate(default_dash(&row.language), widths.lang),
        format_relative_updated(row.updated_at),
        truncate(default_dash(&row.description), widths.description),
        slug = widths.slug,
        stars = widths.stars,
        lang = widths.lang,
        updated = widths.updated,
    )
    .expect("write search row");
}

pub fn format_stars(stars: u64) -> String {
    if stars < 1_000 {
        return stars.to_string();
    }
    if stars < 1_000_000 {
        return format_with_suffix(stars, 1_000.0, "k");
    }
    format_with_suffix(stars, 1_000_000.0, "m")
}

fn format_with_suffix(value: u64, divisor: f64, suffix: &str) -> String {
    let scaled = value as f64 / divisor;
    if scaled >= 100.0 || (scaled.fract() == 0.0) {
        format!("{:.0}{suffix}", scaled)
    } else {
        format!("{:.1}{suffix}", scaled)
    }
}

pub fn format_relative_updated(updated_at: DateTime<Utc>) -> String {
    format_relative_updated_with_now(updated_at, Utc::now())
}

fn format_relative_updated_with_now(updated_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = now.signed_duration_since(updated_at);
    if delta.num_seconds() < 0 {
        return "today".to_string();
    }

    let days = delta.num_days();
    if days == 0 {
        return "today".to_string();
    }
    if days < 30 {
        return if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{days} days ago")
        };
    }

    let months = days / 30;
    if months < 12 {
        return if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{months} months ago")
        };
    }

    let years = months / 12;
    if years == 1 {
        "1 year ago".to_string()
    } else {
        format!("{years} years ago")
    }
}

pub fn default_dash(value: &str) -> &str {
    if value.trim().is_empty() { "-" } else { value }
}

pub fn truncate(value: &str, max: usize) -> String {
    output::truncate_end(value, max)
}

struct SearchColumnWidths {
    slug: usize,
    stars: usize,
    lang: usize,
    updated: usize,
    description: usize,
}

impl SearchColumnWidths {
    fn from_rows(rows: &[RepositorySearchResult]) -> Self {
        let slug = rows
            .iter()
            .map(|r| r.repo_slug.chars().count())
            .max()
            .unwrap_or(4)
            .max("Slug".len())
            .min(36);

        let stars = rows
            .iter()
            .map(|r| format_stars(r.stars).chars().count())
            .max()
            .unwrap_or(5)
            .max("Stars".len());

        let lang = rows
            .iter()
            .map(|r| default_dash(&r.language).chars().count())
            .max()
            .unwrap_or(4)
            .max("Lang".len())
            .min(14);

        let updated = rows
            .iter()
            .map(|r| format_relative_updated(r.updated_at).chars().count())
            .max()
            .unwrap_or(10)
            .max("Updated".len())
            .max("2 years ago".len());

        let description = 72;

        Self {
            slug,
            stars,
            lang,
            updated,
            description,
        }
    }

    fn table_width(&self) -> usize {
        self.slug + self.stars + self.lang + self.updated + self.description + 4
    }
}

#[cfg(test)]
mod tests {
    use super::{format_relative_updated_with_now, format_stars, json_search_result, truncate};
    use crate::models::{
        common::enums::Provider,
        provider::{RepositorySearchFilters, RepositorySearchResult},
    };
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn format_stars_uses_compact_suffixes() {
        assert_eq!(format_stars(561), "561");
        assert_eq!(format_stars(1_000), "1k");
        assert_eq!(format_stars(9_645), "9.6k");
        assert_eq!(format_stars(63_520), "63.5k");
        assert_eq!(format_stars(1_250_000), "1.2m");
    }

    #[test]
    fn format_relative_updated_uses_readable_buckets() {
        let now = Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap();
        assert_eq!(format_relative_updated_with_now(now, now), "today");
        assert_eq!(
            format_relative_updated_with_now(now - Duration::days(2), now),
            "2 days ago"
        );
        assert_eq!(
            format_relative_updated_with_now(now - Duration::days(65), now),
            "2 months ago"
        );
        assert_eq!(
            format_relative_updated_with_now(now - Duration::days(800), now),
            "2 years ago"
        );
    }

    #[test]
    fn truncate_adds_ellipsis_at_fixed_width() {
        let value = "ripgrep recursively searches directories for regex patterns";
        let t1 = truncate(value, 32);
        let t2 = truncate(value, 32);
        assert_eq!(t1, t2);
        assert!(t1.ends_with("..."));
        assert_eq!(t1.chars().count(), 32);
    }

    #[test]
    fn json_search_result_preserves_repository_fields() {
        let updated_at = Utc.with_ymd_and_hms(2026, 6, 12, 1, 2, 3).unwrap();
        let result = json_search_result(
            "ripgrep",
            &Provider::Github,
            None,
            10,
            &RepositorySearchFilters::new(
                Some("Rust".to_string()),
                Some("cli".to_string()),
                Some(100),
                Some(100_000),
                None,
                false,
                false,
            ),
            &[RepositorySearchResult {
                repo_slug: "BurntSushi/ripgrep".to_string(),
                display_name: "ripgrep".to_string(),
                description: "search tool".to_string(),
                stars: 51_000,
                language: "Rust".to_string(),
                updated_at,
            }],
        );
        let json = serde_json::to_value(result).expect("serialize search result");

        assert_eq!(json["query"], "ripgrep");
        assert_eq!(json["provider"], "github");
        assert_eq!(json["results"][0]["repo_slug"], "BurntSushi/ripgrep");
        assert_eq!(
            json["results"][0]["updated_at"],
            "2026-06-12T01:02:03+00:00"
        );
        assert_eq!(json["filters"]["language"], "Rust");
        assert_eq!(json["filters"]["topic"], "cli");
        assert_eq!(json["filters"]["min_stars"], 100);
        assert_eq!(json["filters"]["max_stars"], 100_000);
        assert_eq!(json["filters"]["include_archived"], false);
    }
}
