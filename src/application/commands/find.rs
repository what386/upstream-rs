use anyhow::Result;
use std::fmt::Write as _;

use crate::{
    application::commands::{install, search},
    models::{
        common::enums::{Channel, Filetype, Provider, TrustMode},
        provider::RepositorySearchResult,
    },
    output,
};

#[allow(clippy::too_many_arguments)]
pub async fn run(
    query_words: Vec<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    limit: u32,
    name: Option<String>,
    kind: Filetype,
    channel: Channel,
    match_pattern: Option<String>,
    exclude_pattern: Option<String>,
    desktop: bool,
    trust_mode: TrustMode,
    dry_run: bool,
) -> Result<()> {
    let query = query_words.join(" ").trim().to_string();
    if query.is_empty() {
        println!("{}", output::warning("Search query cannot be empty."));
        return Ok(());
    }

    let search = search::search_repositories(query, provider, base_url, limit).await?;
    if search.results.is_empty() {
        println!("{}", output::warning("No repositories found."));
        return Ok(());
    }

    let choices = SearchChoiceTable::from_results(&search.results);
    let prompt = format!("Find: '{}' via {}", search.query, search.provider);

    let Some(selected) = output::select_from_table(prompt, &choices.headers, &choices.rows)? else {
        println!("{}", output::warning("Cancelled"));
        return Ok(());
    };

    let result = &search.results[selected];
    let inferred_name = default_package_name(result);
    let install_name = match name {
        Some(name) => name,
        None => output::prompt_text("Package name", Some(&inferred_name))?,
    };
    println!(
        "{}",
        output::title(format!("Selected {} as {}", result.repo_slug, install_name))
    );

    install::run(
        Some(install_name),
        result.repo_slug.clone(),
        kind,
        None,
        Some(search.provider),
        search.base_url,
        channel,
        match_pattern,
        exclude_pattern,
        desktop,
        trust_mode,
        dry_run,
    )
    .await
}

struct SearchChoiceTable {
    headers: Vec<String>,
    rows: Vec<String>,
}

impl SearchChoiceTable {
    fn from_results(results: &[RepositorySearchResult]) -> Self {
        let widths = SearchChoiceWidths::from_rows(results);
        let mut header = String::new();
        write!(
            header,
            "  {:<slug$} {:>stars$} {:<lang$} {:<updated$} Description",
            "Slug",
            "Stars",
            "Lang",
            "Updated",
            slug = widths.slug,
            stars = widths.stars,
            lang = widths.lang,
            updated = widths.updated,
        )
        .expect("write find header");
        let divider = format!("  {}", output::divider(widths.table_width()));

        let rows = results
            .iter()
            .map(|result| format_search_choice(result, &widths))
            .collect();

        Self {
            headers: vec![header, divider],
            rows,
        }
    }
}

fn format_search_choice(result: &RepositorySearchResult, widths: &SearchChoiceWidths) -> String {
    format!(
        "{:<slug$} {:>stars$} {:<lang$} {:<updated$} {}",
        search::truncate(&result.repo_slug, widths.slug),
        search::format_stars(result.stars),
        search::truncate(search::default_dash(&result.language), widths.lang),
        search::format_relative_updated(result.updated_at),
        search::truncate(
            search::default_dash(&result.description),
            widths.description
        ),
        slug = widths.slug,
        stars = widths.stars,
        lang = widths.lang,
        updated = widths.updated,
    )
}

struct SearchChoiceWidths {
    slug: usize,
    stars: usize,
    lang: usize,
    updated: usize,
    description: usize,
}

impl SearchChoiceWidths {
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
            .map(|r| search::format_stars(r.stars).chars().count())
            .max()
            .unwrap_or(5)
            .max("Stars".len());

        let lang = rows
            .iter()
            .map(|r| search::default_dash(&r.language).chars().count())
            .max()
            .unwrap_or(4)
            .max("Lang".len())
            .min(14);

        let updated = rows
            .iter()
            .map(|r| {
                search::format_relative_updated(r.updated_at)
                    .chars()
                    .count()
            })
            .max()
            .unwrap_or(10)
            .max("Updated".len())
            .max("2 years ago".len());

        Self {
            slug,
            stars,
            lang,
            updated,
            description: 72,
        }
    }

    fn table_width(&self) -> usize {
        self.slug + self.stars + self.lang + self.updated + self.description + 4
    }
}

fn default_package_name(result: &RepositorySearchResult) -> String {
    let candidate = result
        .repo_slug
        .rsplit('/')
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&result.display_name);

    sanitize_package_name(candidate)
}

fn sanitize_package_name(value: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;

    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            previous_dash = false;
        } else if !previous_dash && !out.is_empty() {
            out.push('-');
            previous_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "package".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchChoiceTable, default_package_name, sanitize_package_name};
    use crate::models::provider::RepositorySearchResult;
    use chrono::{TimeZone, Utc};

    fn result(repo_slug: &str, display_name: &str) -> RepositorySearchResult {
        RepositorySearchResult {
            repo_slug: repo_slug.to_string(),
            display_name: display_name.to_string(),
            description: "fast search".to_string(),
            stars: 51_000,
            language: "Rust".to_string(),
            updated_at: Utc.with_ymd_and_hms(2026, 6, 13, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn default_package_name_uses_repo_basename() {
        assert_eq!(
            default_package_name(&result("BurntSushi/ripgrep", "ripgrep")),
            "ripgrep"
        );
    }

    #[test]
    fn sanitize_package_name_keeps_alias_shell_friendly() {
        assert_eq!(sanitize_package_name("My Tool_2!"), "my-tool-2");
        assert_eq!(sanitize_package_name("..."), "package");
    }

    #[test]
    fn search_choice_table_contains_aligned_install_relevant_fields() {
        let table = SearchChoiceTable::from_results(&[result("BurntSushi/ripgrep", "ripgrep")]);
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
