#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSection {
    pub level: usize,
    pub heading: String,
    pub body: String,
    pub ordinal: usize,
}

#[derive(Debug, Clone)]
struct Heading {
    level: usize,
    text: String,
    line_index: usize,
}

pub fn parse_sections(markdown: &str) -> Vec<MarkdownSection> {
    let lines = markdown.lines().collect::<Vec<_>>();
    let headings = lines
        .iter()
        .enumerate()
        .filter_map(|(line_index, line)| {
            parse_heading(line).map(|(level, text)| Heading {
                level,
                text,
                line_index,
            })
        })
        .collect::<Vec<_>>();

    headings
        .iter()
        .enumerate()
        .map(|(ordinal, heading)| {
            let body_start = heading.line_index + 1;
            let body_end = next_sibling_or_parent_index(&headings, ordinal).unwrap_or(lines.len());
            let body = lines[body_start..body_end].join("\n").trim().to_string();

            MarkdownSection {
                level: heading.level,
                heading: heading.text.clone(),
                body,
                ordinal,
            }
        })
        .collect()
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 || level > 6 {
        return None;
    }

    let rest = &trimmed[level..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }

    let heading = rest.trim().trim_end_matches('#').trim();
    (!heading.is_empty()).then(|| (level, heading.to_string()))
}

fn next_sibling_or_parent_index(headings: &[Heading], current: usize) -> Option<usize> {
    let current_level = headings[current].level;
    headings
        .iter()
        .skip(current + 1)
        .find(|heading| heading.level <= current_level)
        .map(|heading| heading.line_index)
}

#[cfg(test)]
mod tests {
    use super::parse_sections;

    #[test]
    fn parses_markdown_sections_with_nested_body_content() {
        let markdown = "\
# Project
intro

## Usage
basic usage

### Flags
flag details

## Install
install notes
";

        let sections = parse_sections(markdown);

        assert_eq!(sections.len(), 4);
        assert_eq!(sections[1].heading, "Usage");
        assert!(sections[1].body.contains("basic usage"));
        assert!(sections[1].body.contains("### Flags"));
        assert!(sections[1].body.contains("flag details"));
        assert!(!sections[1].body.contains("## Install"));
    }

    #[test]
    fn parses_this_projects_readme() {
        let sections = parse_sections(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/README.md"
        )));

        assert!(sections.iter().any(|section| section.heading == "Upstream"));
        assert!(
            sections
                .iter()
                .any(|section| section.heading == "Command Overview")
        );
        assert!(
            sections
                .iter()
                .any(|section| section.heading == "Documentation")
        );
    }
}
