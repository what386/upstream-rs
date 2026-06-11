use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::{
    models::{common::enums::Provider, provider::RepositorySearchResult},
    output,
    output::pager,
    providers::provider_manager::ProviderManager,
    services::storage::config_storage::ConfigStorage,
    utils::static_paths::UpstreamPaths,
};
use std::fmt::Write as _;

pub async fn run(
    query_words: Vec<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    limit: u32,
) -> Result<()> {
    let query = query_words.join(" ").trim().to_string();
    if query.is_empty() {
        println!("{}", output::warning("Search query cannot be empty."));
        return Ok(());
    }

    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let app_config = config.get_config();

    let github_token = app_config.github.api_token.as_deref();
    let gitlab_token = app_config.gitlab.api_token.as_deref();
    let gitea_token = app_config.gitea.api_token.as_deref();

    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;
    let effective_provider = provider.unwrap_or(Provider::Github);

    let results = provider_manager
        .search_repositories(
            &query,
            &effective_provider,
            Some(limit.max(1)),
            base_url.as_deref(),
        )
        .await?;

    if results.is_empty() {
        println!("{}", output::warning("No repositories found."));
        return Ok(());
    }

    let title = format!("Search: '{}' via {}", query, effective_provider);
    pager::page_text(Some(&title), &format_results(&results))?;
    Ok(())
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

fn format_stars(stars: u64) -> String {
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

fn format_relative_updated(updated_at: DateTime<Utc>) -> String {
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

fn default_dash(value: &str) -> &str {
    if value.trim().is_empty() { "-" } else { value }
}

fn truncate(value: &str, max: usize) -> String {
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
    use super::{format_relative_updated_with_now, format_stars, truncate};
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
}
