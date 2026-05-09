use anyhow::Result;
use console::style;

use crate::{
    models::{common::enums::Provider, provider::RepositorySearchResult},
    providers::provider_manager::ProviderManager,
    services::storage::config_storage::ConfigStorage,
    utils::static_paths::UpstreamPaths,
};

pub async fn run(
    query_words: Vec<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    limit: u32,
) -> Result<()> {
    let query = query_words.join(" ").trim().to_string();
    if query.is_empty() {
        println!("Search query cannot be empty.");
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

    println!(
        "{}",
        style(format!("Searching '{}' via {} ...", query, effective_provider)).cyan()
    );

    let results = provider_manager
        .search_repositories(
            &query,
            &effective_provider,
            Some(limit.max(1)),
            base_url.as_deref(),
        )
        .await?;

    if results.is_empty() {
        println!("No repositories found.");
        return Ok(());
    }

    print_results(&results);
    Ok(())
}

fn print_results(results: &[RepositorySearchResult]) {
    let widths = SearchColumnWidths::from_rows(results);

    println!(
        "{:<slug$} {:>stars$} {:<lang$} {:<updated$} {}",
        "Slug",
        "Stars",
        "Lang",
        "Updated",
        "Description",
        slug = widths.slug,
        stars = widths.stars,
        lang = widths.lang,
        updated = widths.updated,
    );

    for row in results {
        println!(
            "{:<slug$} {:>stars$} {:<lang$} {:<updated$} {}",
            truncate(&row.repo_slug, widths.slug),
            row.stars,
            truncate(default_dash(&row.language), widths.lang),
            row.updated_at.format("%Y-%m-%d"),
            truncate(default_dash(&row.description), widths.description),
            slug = widths.slug,
            stars = widths.stars,
            lang = widths.lang,
            updated = widths.updated,
        );
    }
}

fn default_dash(value: &str) -> &str {
    if value.trim().is_empty() { "-" } else { value }
}

fn truncate(value: &str, max: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max {
        return value.to_string();
    }

    let mut out = String::new();
    for ch in value.chars().take(max.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    out
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
            .map(|r| r.stars.to_string().chars().count())
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

        let updated = "Updated".len().max(10);

        let description = rows
            .iter()
            .map(|r| default_dash(&r.description).chars().count())
            .max()
            .unwrap_or(11)
            .max("Description".len())
            .min(64);

        Self {
            slug,
            stars,
            lang,
            updated,
            description,
        }
    }
}
