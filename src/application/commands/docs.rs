use std::fmt::Write as _;

use anyhow::{Result, anyhow};

use crate::{
    application::context::CommandContext,
    output,
    output::pager,
    routines::docs::{self, DocsSearchResult, DocsSectionMatch, ProjectReadmeSource},
};

pub async fn run(name: String, keywords: Vec<String>, offline: bool) -> Result<()> {
    let context = CommandContext::new()?;
    let package_storage = context.package_storage()?;
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
            result.package_name, result.document_name
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
    writeln!(out, "queries: {}", result.query).expect("write docs query");
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

#[cfg(test)]
mod tests {
    use super::{DocsChoiceTable, format_selected_section};
    use crate::{
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
}
