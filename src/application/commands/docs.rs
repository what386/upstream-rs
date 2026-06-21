use std::fmt::Write as _;

use anyhow::{Result, anyhow};

use crate::{application::context::CommandContext, output, output::pager};

pub fn run(name: String, keywords: Vec<String>) -> Result<()> {
    let context = CommandContext::new()?;
    let package_storage = context.package_storage()?;
    let package = package_storage
        .get_package_by_name(&name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed", name))?;

    let query = keywords.join(" ").trim().to_string();
    let sections = placeholder_sections(&query);
    let choices = DocsChoiceTable::from_sections(&package.name, &query, &sections);

    let Some(selected) = output::select_from_table_with_preview(
        format!(
            "package: {}  doc: README.md\nqueries: {query}",
            package.name
        ),
        &choices.headers,
        &choices.rows,
        &choices.previews,
    )?
    else {
        println!("{}", output::warning("Cancelled"));
        return Ok(());
    };

    let text = format_selected_section(&package.name, &query, &sections[selected]);
    pager::page_text(None, &text)?;
    Ok(())
}

fn placeholder_sections(query: &str) -> Vec<PlaceholderSection> {
    let query = query.to_ascii_lowercase();
    let mut sections = vec![
        PlaceholderSection::new("Usage", "Basic command forms and common invocations.", 0.72),
        PlaceholderSection::new("Examples", "Worked examples from the package README.", 0.66),
        PlaceholderSection::new(
            "Configuration",
            "Configuration files, flags, and environment variables.",
            0.58,
        ),
        PlaceholderSection::new(
            "Installation",
            "Install notes and platform-specific requirements.",
            0.51,
        ),
        PlaceholderSection::new(
            "Frequently Asked Questions",
            "Troubleshooting and common project caveats.",
            0.44,
        ),
    ];

    for section in &mut sections {
        let haystack = format!(
            "{} {}",
            section.heading.to_ascii_lowercase(),
            section.summary.to_ascii_lowercase()
        );
        let hits = query
            .split_whitespace()
            .filter(|keyword| haystack.contains(keyword))
            .count();
        section.score = (section.score + hits as f32 * 0.12).min(0.98);
    }

    sections.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.heading.cmp(right.heading))
    });
    sections
}

struct DocsChoiceTable {
    headers: Vec<String>,
    rows: Vec<String>,
    previews: Vec<String>,
}

impl DocsChoiceTable {
    fn from_sections(_package_name: &str, _query: &str, sections: &[PlaceholderSection]) -> Self {
        let headers = vec![format!("{:<7} Section", "Score"), output::divider(48)];

        let rows = sections
            .iter()
            .map(|section| format!("{:<7} {}", format!("{:.2}", section.score), section.heading))
            .collect();
        let previews = sections
            .iter()
            .map(|section| format_section_preview(section))
            .collect();

        Self {
            headers,
            rows,
            previews,
        }
    }
}

fn format_section_preview(section: &PlaceholderSection) -> String {
    format!(
        "## {}\n\n{}\n\nThis is placeholder content for the planned cached README section search.",
        section.heading, section.summary
    )
}

fn format_selected_section(
    package_name: &str,
    query: &str,
    section: &PlaceholderSection,
) -> String {
    let mut out = String::new();

    writeln!(out, "package: {package_name}  doc: README.md").expect("write docs package");
    writeln!(out, "queries: {query}").expect("write docs query");
    writeln!(out).expect("write docs spacer");
    writeln!(
        out,
        "section: {}  score: {:.2}",
        section.heading, section.score
    )
    .expect("write docs selected section");
    writeln!(out).expect("write docs spacer");
    writeln!(out, "{}", format_section_preview(section)).expect("write docs preview");
    writeln!(out).expect("write docs spacer");
    writeln!(
        out,
        "The real backend will fetch/cache README.md, split markdown sections, score matches, and open the selected section here."
    )
    .expect("write docs placeholder backend");

    out
}

#[derive(Debug, Clone)]
struct PlaceholderSection {
    heading: &'static str,
    summary: &'static str,
    score: f32,
}

impl PlaceholderSection {
    fn new(heading: &'static str, summary: &'static str, score: f32) -> Self {
        Self {
            heading,
            summary,
            score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DocsChoiceTable, format_selected_section, placeholder_sections};

    #[test]
    fn placeholder_sections_prioritize_matching_headings() {
        let sections = placeholder_sections("configuration file");

        assert_eq!(sections[0].heading, "Configuration");
    }

    #[test]
    fn placeholder_output_keeps_header_compact() {
        let sections = placeholder_sections("usage");

        let output = format_selected_section("rg", "usage", &sections[0]);

        assert!(output.contains("package: rg  doc: README.md"));
        assert!(output.contains("queries: usage"));
        assert!(!output.contains("Document:"));
        assert!(!output.contains("Query:"));
        assert!(!output.contains("Source:"));
        assert!(!output.contains("Cache:"));
        assert!(!output.contains("Status:"));
        assert!(output.contains("placeholder content"));
    }

    #[test]
    fn docs_choice_table_pairs_rows_with_previews() {
        let sections = placeholder_sections("usage");
        let table = DocsChoiceTable::from_sections("rg", "usage", &sections);

        assert_eq!(table.rows.len(), sections.len());
        assert_eq!(table.previews.len(), sections.len());
        assert!(table.headers[0].contains("Score"));
        assert!(table.headers[0].contains("Section"));
        assert!(table.rows[0].contains(sections[0].heading));
        assert!(table.previews[0].contains(sections[0].summary));
    }
}
