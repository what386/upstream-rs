use std::cmp::Ordering;

use super::markdown::{MarkdownSection, parse_sections};

#[derive(Debug, Clone, PartialEq)]
pub struct DocsSearchResult {
    pub package_name: String,
    pub document_name: String,
    pub query: String,
    pub sections: Vec<DocsSectionMatch>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocsSectionMatch {
    pub level: usize,
    pub heading: String,
    pub body: String,
    pub score: f32,
    pub ordinal: usize,
}

pub fn search_readme(
    package_name: &str,
    document_name: &str,
    query: &str,
    readme: &str,
) -> DocsSearchResult {
    let keywords = keywords(query);
    let mut sections = parse_sections(readme)
        .into_iter()
        .map(|section| score_section(section, &keywords))
        .collect::<Vec<_>>();

    sections.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.ordinal.cmp(&right.ordinal))
    });

    DocsSearchResult {
        package_name: package_name.to_string(),
        document_name: document_name.to_string(),
        query: query.to_string(),
        sections,
    }
}

fn score_section(section: MarkdownSection, keywords: &[String]) -> DocsSectionMatch {
    let score = score_text(&section.heading, &section.body, keywords);
    DocsSectionMatch {
        level: section.level,
        heading: section.heading,
        body: section.body,
        score,
        ordinal: section.ordinal,
    }
}

fn score_text(heading: &str, body: &str, keywords: &[String]) -> f32 {
    if keywords.is_empty() {
        return 0.0;
    }

    let heading = heading.to_ascii_lowercase();
    let body = body.to_ascii_lowercase();
    let keyword_count = keywords.len() as f32;
    let heading_matches = keywords
        .iter()
        .filter(|keyword| heading.contains(keyword.as_str()))
        .count() as f32;
    let body_matches = keywords
        .iter()
        .filter(|keyword| body.contains(keyword.as_str()))
        .count() as f32;
    let occurrences = keywords
        .iter()
        .map(|keyword| body.matches(keyword.as_str()).count())
        .sum::<usize>() as f32;

    let heading_score = heading_matches / keyword_count * 0.72;
    let body_score = body_matches / keyword_count * 0.22;
    let occurrence_boost = (occurrences.min(8.0) / 8.0) * 0.06;

    (heading_score + body_score + occurrence_boost).min(0.99)
}

fn keywords(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|keyword| {
            keyword
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
                .to_ascii_lowercase()
        })
        .filter(|keyword| !keyword.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::search_readme;

    #[test]
    fn search_prioritizes_matching_heading() {
        let readme = "\
# Tool

## Installation
Download the binary.

## Configuration
Set the config file.
";

        let result = search_readme("tool", "README.md", "configuration file", readme);

        assert_eq!(result.sections[0].heading, "Configuration");
        assert!(result.sections[0].score > result.sections[1].score);
    }

    #[test]
    fn empty_query_returns_sections_in_document_order() {
        let readme = "\
# Tool

## Installation
Download the binary.

## Usage
Run the binary.
";

        let result = search_readme("tool", "README.md", "", readme);

        assert_eq!(
            result
                .sections
                .iter()
                .map(|section| section.heading.as_str())
                .collect::<Vec<_>>(),
            vec!["Tool", "Installation", "Usage"]
        );
        assert!(result.sections.iter().all(|section| section.score == 0.0));
    }

    #[test]
    fn searches_this_projects_readme_sections() {
        let result = search_readme(
            "upstream",
            "README.md",
            "command overview",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")),
        );

        assert_eq!(result.document_name, "README.md");
        assert_eq!(result.sections[0].heading, "Command Overview");
        assert!(result.sections[0].body.contains("install"));
    }
}
