use std::{
    fmt::Write as _,
    io::Write as _,
    process::{Command, Stdio},
};

use anyhow::{Result, anyhow};
use console::Term;

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

    let renderer = MarkdownRenderer::new(terminal_width());
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
    fn from_result(result: &DocsSearchResult, renderer: &MarkdownRenderer) -> Self {
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

struct MarkdownRenderer {
    enabled: bool,
    width: usize,
    command: String,
    style: String,
}

impl MarkdownRenderer {
    fn new(width: usize) -> Self {
        let command = std::env::var("UPSTREAM_GLOW_COMMAND").unwrap_or_else(|_| "glow".to_string());
        Self {
            enabled: glow_is_available(&command),
            width,
            command,
            style: std::env::var("UPSTREAM_GLOW_STYLE").unwrap_or_else(|_| "dark".to_string()),
        }
    }

    #[cfg(test)]
    fn plain() -> Self {
        Self {
            enabled: false,
            width: 80,
            command: "glow".to_string(),
            style: "dark".to_string(),
        }
    }

    fn render(&self, markdown: &str) -> String {
        if !self.enabled {
            return markdown.to_string();
        }

        self.render_with_glow(markdown)
            .filter(|output| !output.trim().is_empty())
            .unwrap_or_else(|| markdown.to_string())
    }

    fn render_with_glow(&self, markdown: &str) -> Option<String> {
        let mut child = Command::new(&self.command)
            .arg("-s")
            .arg(&self.style)
            .arg("-w")
            .arg(self.width.to_string())
            .arg("-n")
            .arg("-")
            .env_remove("NO_COLOR")
            .env("CLICOLOR_FORCE", "1")
            .env("FORCE_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        child.stdin.as_mut()?.write_all(markdown.as_bytes()).ok()?;
        drop(child.stdin.take());

        let output = child.wait_with_output().ok()?;
        if !output.status.success() {
            return None;
        }

        String::from_utf8(output.stdout)
            .ok()
            .map(normalize_glow_output)
    }
}

fn glow_is_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn normalize_glow_output(output: String) -> String {
    let mut lines = output.lines().collect::<Vec<_>>();

    while lines
        .first()
        .is_some_and(|line| console::strip_ansi_codes(line).trim().is_empty())
    {
        lines.remove(0);
    }
    while lines
        .last()
        .is_some_and(|line| console::strip_ansi_codes(line).trim().is_empty())
    {
        lines.pop();
    }

    lines
        .into_iter()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn terminal_width() -> usize {
    let (_, cols) = Term::stdout().size();
    (cols as usize).max(20)
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
    renderer: &MarkdownRenderer,
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
    use super::{DocsChoiceTable, MarkdownRenderer, format_selected_section};
    use crate::routines::docs::{DocsSearchResult, DocsSectionMatch};

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

    #[test]
    fn markdown_renderer_falls_back_when_glow_is_missing() {
        let renderer = MarkdownRenderer {
            enabled: true,
            width: 80,
            command: "upstream-test-missing-glow-command".to_string(),
            style: "dark".to_string(),
        };

        assert_eq!(renderer.render("# Heading\n"), "# Heading\n");
    }

    #[test]
    fn normalize_glow_output_trims_outer_blank_padding() {
        let output = "\n\x1b[1mTitle\x1b[0m   \n\nbody   \n\n".to_string();

        assert_eq!(
            super::normalize_glow_output(output),
            "\x1b[1mTitle\x1b[0m\n\nbody"
        );
    }
}
