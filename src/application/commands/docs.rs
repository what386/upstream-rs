use std::fmt::Write as _;

use anyhow::{Result, anyhow, bail};

use crate::{
    application::context::CommandContext,
    models::upstream::Package,
    output,
    output::pager,
    routines::docs::{self, DocsSearchResult, DocsSectionMatch, ProjectReadmeSource},
};

pub async fn run(
    name: Option<String>,
    keywords: Vec<String>,
    offline: bool,
    fetch: Option<Vec<String>>,
) -> Result<()> {
    let context = CommandContext::new()?;
    let package_storage = context.package_storage()?;
    if let Some(fetch_names) = fetch {
        return run_fetch_readmes(
            &context,
            package_storage.get_all_packages(),
            name,
            keywords,
            offline,
            fetch_names,
        )
        .await;
    }

    let name = name.ok_or_else(|| anyhow!("Package name is required unless --fetch is used"))?;
    let package = package_storage
        .get_package_by_name(&name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed", name))?;

    let query = keywords.join(" ").trim().to_string();
    let result = docs::run(
        &context.provider_manager,
        &context.paths,
        package,
        &query,
        offline,
    )
    .await?;
    if matches!(result.readme_source, ProjectReadmeSource::CachedFallback) {
        println!(
            "{}",
            output::warning("README fetch failed; using cached README.md")
        );
    }
    let result = result.search;
    if result.sections.is_empty() {
        println!("{}", output::warning("No README sections found."));
        return Ok(());
    }

    let renderer = output::MarkdownRenderer::for_terminal();
    let choices = DocsChoiceTable::from_result(&result, &renderer);

    let Some(selected) = output::select_from_table_with_preview(
        format!(
            "package: {}  doc: {}\nqueries: {query}",
            result.package_name,
            result.document_name,
            query = query_label(&result.query)
        ),
        &choices.headers,
        &choices.rows,
        &choices.previews,
    )?
    else {
        println!("{}", output::warning("Cancelled"));
        return Ok(());
    };

    let text = format_selected_section(&result, &result.sections[selected], &renderer);
    pager::page_text(None, &text)?;
    Ok(())
}

async fn run_fetch_readmes(
    context: &CommandContext,
    packages: &[Package],
    leading_name: Option<String>,
    keywords: Vec<String>,
    offline: bool,
    fetch_names: Vec<String>,
) -> Result<()> {
    if offline {
        bail!("--offline cannot be used with --fetch");
    }

    let targets = resolve_fetch_targets(packages, leading_name, keywords, fetch_names)?;
    if targets.is_empty() {
        println!("{}", output::warning("No installed packages to refresh."));
        return Ok(());
    }

    println!("{}", output::title("Refreshing README docs"));
    let width = output::status_subject_width(targets.iter().map(|package| package.name.as_str()));
    let mut failures = 0usize;

    for package in &targets {
        match docs::refetch_project_readme(
            &context.provider_manager,
            &context.paths.dirs.cache_dir,
            package,
        )
        .await
        {
            Ok(_) => println!(
                "{}",
                output::status_line_text_with_width(
                    output::Status::Ok,
                    &package.name,
                    "cached README.md",
                    width,
                )
            ),
            Err(err) => {
                failures += 1;
                println!(
                    "{}",
                    output::status_line_text_with_width(
                        output::Status::Fail,
                        &package.name,
                        output::error_summary(&err),
                        width,
                    )
                );
            }
        }
    }

    if failures > 0 {
        bail!("Failed to refresh {failures}/{} README docs", targets.len());
    }

    Ok(())
}

fn resolve_fetch_targets(
    packages: &[Package],
    leading_name: Option<String>,
    keywords: Vec<String>,
    fetch_names: Vec<String>,
) -> Result<Vec<Package>> {
    if !fetch_names.is_empty() {
        if leading_name.is_some() || !keywords.is_empty() {
            bail!("When using --fetch with package names, pass names after --fetch");
        }

        return packages_for_names(packages, &fetch_names);
    }

    if !keywords.is_empty() {
        bail!("docs --fetch does not accept search keywords");
    }

    if let Some(name) = leading_name {
        return packages_for_names(packages, &[name]);
    }

    let mut targets = packages.to_vec();
    targets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(targets)
}

fn packages_for_names(packages: &[Package], names: &[String]) -> Result<Vec<Package>> {
    names
        .iter()
        .map(|name| {
            packages
                .iter()
                .find(|package| package.name == *name)
                .cloned()
                .ok_or_else(|| anyhow!("Package '{}' is not installed", name))
        })
        .collect()
}

struct DocsChoiceTable {
    headers: Vec<String>,
    rows: Vec<String>,
    previews: Vec<String>,
}

impl DocsChoiceTable {
    fn from_result(result: &DocsSearchResult, renderer: &output::MarkdownRenderer) -> Self {
        let headers = vec![format!("{:<7} Section", "Score"), output::divider(48)];

        let rows = result
            .sections
            .iter()
            .map(|section| {
                format!(
                    "{:<7} {}",
                    format!("{:.2}", section.score),
                    section_heading_label(section)
                )
            })
            .collect();
        let previews = result
            .sections
            .iter()
            .map(|section| renderer.render(&format_section_preview(section)))
            .collect();

        Self {
            headers,
            rows,
            previews,
        }
    }
}

fn section_heading_label(section: &DocsSectionMatch) -> String {
    format!(
        "{}{}",
        "  ".repeat(section.level.saturating_sub(1)),
        section.heading
    )
}

fn format_section_preview(section: &DocsSectionMatch) -> String {
    let body = preview_body(&section.body);
    format!(
        "{} {}\n\n{}",
        "#".repeat(section.level.clamp(1, 6)),
        section.heading,
        body
    )
}

fn preview_body(body: &str) -> String {
    let preview = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("\n");

    if preview.is_empty() {
        "(no section content)".to_string()
    } else {
        preview
    }
}

fn format_selected_section(
    result: &DocsSearchResult,
    section: &DocsSectionMatch,
    renderer: &output::MarkdownRenderer,
) -> String {
    let mut out = String::new();
    let section_markdown = format_section_markdown(section);

    writeln!(
        out,
        "package: {}  doc: {}",
        result.package_name, result.document_name
    )
    .expect("write docs package");
    writeln!(out, "queries: {}", query_label(&result.query)).expect("write docs query");
    writeln!(out).expect("write docs spacer");
    writeln!(
        out,
        "section: {}  score: {:.2}",
        section.heading, section.score
    )
    .expect("write docs selected section");
    writeln!(out).expect("write docs spacer");

    out.push_str(&renderer.render(&section_markdown));
    if !out.ends_with('\n') {
        out.push('\n');
    }

    out
}

fn format_section_markdown(section: &DocsSectionMatch) -> String {
    let mut out = String::new();

    writeln!(
        out,
        "{} {}",
        "#".repeat(section.level.clamp(1, 6)),
        section.heading
    )
    .expect("write docs heading");
    writeln!(out).expect("write docs spacer");
    writeln!(out, "{}", section.body).expect("write docs body");

    out
}

fn query_label(query: &str) -> &str {
    if query.trim().is_empty() {
        "(none)"
    } else {
        query
    }
}

#[cfg(test)]
mod tests {
    use super::{DocsChoiceTable, format_selected_section, query_label, resolve_fetch_targets};
    use crate::{
        models::{
            common::enums::{Channel, Filetype, Provider},
            upstream::Package,
        },
        output::MarkdownRenderer,
        routines::docs::{DocsSearchResult, DocsSectionMatch},
    };

    fn result() -> DocsSearchResult {
        DocsSearchResult {
            package_name: "rg".to_string(),
            document_name: "README.md".to_string(),
            query: "usage".to_string(),
            sections: vec![DocsSectionMatch {
                level: 2,
                heading: "Usage".to_string(),
                body: "Basic usage notes.".to_string(),
                score: 0.94,
                ordinal: 1,
            }],
        }
    }

    fn package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn selected_section_output_keeps_header_compact() {
        let result = result();
        let renderer = MarkdownRenderer::plain();

        let output = format_selected_section(&result, &result.sections[0], &renderer);

        assert!(output.contains("package: rg  doc: README.md"));
        assert!(output.contains("queries: usage"));
        assert!(!output.contains("Document:"));
        assert!(!output.contains("Query:"));
        assert!(!output.contains("Source:"));
        assert!(!output.contains("Cache:"));
        assert!(!output.contains("Status:"));
        assert!(output.contains("Basic usage notes."));
    }

    #[test]
    fn selected_section_output_labels_empty_query() {
        let mut result = result();
        result.query.clear();
        let renderer = MarkdownRenderer::plain();

        let output = format_selected_section(&result, &result.sections[0], &renderer);

        assert!(output.contains("queries: (none)"));
    }

    #[test]
    fn docs_choice_table_pairs_rows_with_previews() {
        let result = result();
        let renderer = MarkdownRenderer::plain();
        let table = DocsChoiceTable::from_result(&result, &renderer);

        assert_eq!(table.rows.len(), result.sections.len());
        assert_eq!(table.previews.len(), result.sections.len());
        assert!(table.headers[0].contains("Score"));
        assert!(table.headers[0].contains("Section"));
        assert!(table.rows[0].contains("Usage"));
        assert!(table.previews[0].contains("Basic usage notes."));
    }

    #[test]
    fn query_label_uses_none_for_empty_query() {
        assert_eq!(query_label(""), "(none)");
        assert_eq!(query_label("   "), "(none)");
        assert_eq!(query_label("usage"), "usage");
    }

    #[test]
    fn fetch_targets_all_packages_when_names_are_omitted() {
        let packages = vec![package("zoxide"), package("bat")];

        let targets =
            resolve_fetch_targets(&packages, None, Vec::new(), Vec::new()).expect("targets");

        assert_eq!(
            targets
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>(),
            vec!["bat", "zoxide"]
        );
    }

    #[test]
    fn fetch_targets_support_single_leading_package_name() {
        let packages = vec![package("rg"), package("bat")];

        let targets =
            resolve_fetch_targets(&packages, Some("rg".to_string()), Vec::new(), Vec::new())
                .expect("targets");

        assert_eq!(targets[0].name, "rg");
    }

    #[test]
    fn fetch_targets_support_names_after_fetch_flag() {
        let packages = vec![package("rg"), package("bat"), package("fd")];

        let targets = resolve_fetch_targets(
            &packages,
            None,
            Vec::new(),
            vec!["bat".to_string(), "fd".to_string()],
        )
        .expect("targets");

        assert_eq!(
            targets
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>(),
            vec!["bat", "fd"]
        );
    }

    #[test]
    fn fetch_targets_reject_search_keywords() {
        let packages = vec![package("rg")];

        let err = resolve_fetch_targets(
            &packages,
            Some("rg".to_string()),
            vec!["usage".to_string()],
            Vec::new(),
        )
        .expect_err("keywords rejected");

        assert!(err.to_string().contains("search keywords"));
    }
}
