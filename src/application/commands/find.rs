use anyhow::Result;
use chrono::NaiveDate;

use crate::{
    application::commands::{install, search},
    models::{
        common::enums::{Channel, Filetype, Provider, TrustMode},
        provider::{RepositorySearchFilters, RepositorySearchResult},
        upstream::config::AppConfig,
    },
    output,
    utils::static_paths::UpstreamPaths,
};

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
    kind: Filetype,
    channel: Channel,
    match_pattern: Option<String>,
    exclude_pattern: Option<String>,
    desktop: bool,
    trust_mode: TrustMode,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let query = query_words.join(" ").trim().to_string();
    if query.is_empty() {
        println!("{}", output::warning("Search query cannot be empty."));
        return Ok(());
    }

    let search = search::search_repositories(
        query,
        provider,
        base_url,
        limit,
        RepositorySearchFilters::new(
            language,
            topic,
            min_stars,
            max_stars,
            pushed_after,
            include_forks,
            include_archived,
        ),
        paths,
        app_config,
    )
    .await?;

    if search.results.is_empty() {
        println!("{}", output::warning("No repositories found."));
        return Ok(());
    }

    let choices = SearchChoiceTable::from_results(&search.results, &search.provider);
    let prompt = format!("Find: '{}' via {}", search.query, search.provider);

    let Some(selected) = output::select_from_table(prompt, &choices.headers, &choices.rows)? else {
        println!("{}", output::warning("Cancelled"));
        return Ok(());
    };

    let result = &search.results[selected];
    println!(
        "{}",
        output::title(format!("Selected {}", result.repo_slug))
    );

    install::run(
        result.repo_slug.clone(),
        kind,
        None,
        None,
        Some(search.provider),
        search.base_url,
        channel,
        match_pattern,
        exclude_pattern,
        desktop,
        trust_mode,
        dry_run,
        paths,
        app_config,
    )
    .await
}

struct SearchChoiceTable {
    headers: Vec<String>,
    rows: Vec<String>,
}

impl SearchChoiceTable {
    fn from_results(results: &[RepositorySearchResult], provider: &Provider) -> Self {
        let width = (console::Term::stdout().size().1 as usize).saturating_sub(2);
        let table = search::table_for_results(results, provider, width);
        Self {
            headers: vec![
                format!("  {}", table.header),
                format!("  {}", table.divider),
            ],
            rows: table.rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SearchChoiceTable;
    use crate::models::provider::{RepositorySearchResult, SearchMetadataValue};
    use chrono::{TimeZone, Utc};

    fn result(repo_slug: &str, display_name: &str) -> RepositorySearchResult {
        RepositorySearchResult {
            repo_slug: repo_slug.to_string(),
            display_name: display_name.to_string(),
            description: "fast search".to_string(),
            metadata: vec![
                ("stars".into(), SearchMetadataValue::Integer(51_000)),
                ("lang".into(), SearchMetadataValue::Text("Rust".into())),
            ],
            updated_at: Utc.with_ymd_and_hms(2026, 6, 13, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn appimagehub_choices_show_names_and_stable_ids() {
        let mut entry = result("2259793", "Krita");
        entry.metadata.clear();
        let table = SearchChoiceTable::from_results(
            &[entry],
            &crate::models::common::enums::Provider::AppImageHub,
        );

        assert!(table.headers[0].contains("Name / ID"));
        assert!(!table.headers[0].contains("Stars"));
        assert!(!table.headers[0].contains("Lang"));
        assert!(table.rows[0].contains("Krita (2259793)"));
        assert!(!table.rows[0].contains("51k"));
    }

    #[test]
    fn search_choice_table_contains_aligned_install_relevant_fields() {
        let table = SearchChoiceTable::from_results(
            &[result("BurntSushi/ripgrep", "ripgrep")],
            &crate::models::common::enums::Provider::Github,
        );

        let choice = &table.rows[0];

        assert!(table.headers[0].contains("Slug"));
        assert!(table.headers[0].contains("Stars"));
        assert!(table.headers[0].contains("Updated"));
        assert!(table.headers[1].trim().chars().all(|ch| ch == '-'));
        assert!(choice.contains("BurntSushi/ripgrep"));
        assert!(choice.contains("51k"));
        assert!(choice.contains("Rust"));
        assert!(choice.contains("fast search"));
    }
}
