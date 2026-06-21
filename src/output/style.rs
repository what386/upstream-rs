use console::{StyledObject, style};
use std::fmt;

pub fn title(text: impl fmt::Display) -> StyledObject<String> {
    style(text.to_string()).cyan().bold()
}

pub fn section(text: impl fmt::Display) -> StyledObject<String> {
    style(text.to_string()).bold()
}

pub fn meta(text: impl fmt::Display) -> StyledObject<String> {
    style(text.to_string()).dim()
}

pub fn success(text: impl fmt::Display) -> StyledObject<String> {
    style(text.to_string()).green()
}

pub fn warning(text: impl fmt::Display) -> StyledObject<String> {
    style(text.to_string()).yellow()
}

pub fn kv(label: &str, value: impl fmt::Display) {
    println!("  {:<13} {}", meta(format!("{label}:")), value);
}

pub fn action_note(text: impl fmt::Display) {
    println!("  {}", meta(text));
}

pub fn divider(width: usize) -> String {
    "-".repeat(width)
}

pub fn truncate_visible(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }

    let mut chars = value.chars().peekable();
    let mut out = String::new();
    let mut visible = 0;
    let mut saw_escape = false;
    let mut truncated = false;

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            saw_escape = true;
            out.push(ch);
            copy_ansi_escape(&mut chars, &mut out);
            continue;
        }

        if visible >= max {
            truncated = true;
            break;
        }

        out.push(ch);
        visible += 1;
    }

    if saw_escape && truncated {
        out.push_str("\x1b[0m");
    }

    out
}

fn copy_ansi_escape<I>(chars: &mut std::iter::Peekable<I>, out: &mut String)
where
    I: Iterator<Item = char>,
{
    match chars.next() {
        Some('[') => {
            out.push('[');
            for ch in chars.by_ref() {
                out.push(ch);
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        }
        Some(']') => {
            out.push(']');
            while let Some(ch) = chars.next() {
                out.push(ch);
                if ch == '\x07' {
                    break;
                }
                if ch == '\x1b' && chars.peek() == Some(&'\\') {
                    out.push(chars.next().expect("peeked OSC terminator"));
                    break;
                }
            }
        }
        Some(ch) => out.push(ch),
        None => {}
    }
}

pub fn truncate_end(value: &str, max: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max {
        return value.to_string();
    }
    if max <= 3 {
        return ".".repeat(max);
    }

    let mut out = String::new();
    for ch in value.chars().take(max - 3) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

pub fn truncate_middle(value: &str, max: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max {
        return value.to_string();
    }
    if max <= 3 {
        return ".".repeat(max);
    }

    let keep = max - 3;
    let prefix_len = keep / 2;
    let suffix_len = keep - prefix_len;
    let prefix: String = value.chars().take(prefix_len).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(suffix_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::truncate_visible;

    #[test]
    fn truncate_visible_ignores_ansi_sequences() {
        let truncated = truncate_visible("\x1b[31mabcdef\x1b[0m", 3);

        assert_eq!(console::strip_ansi_codes(&truncated), "abc");
        assert!(truncated.starts_with("\x1b[31m"));
        assert!(truncated.ends_with("\x1b[0m"));
    }

    #[test]
    fn truncate_visible_keeps_short_colored_line_intact() {
        let line = "\x1b[1mbold\x1b[0m";

        assert_eq!(truncate_visible(line, 10), line);
    }

    #[test]
    fn truncate_visible_does_not_split_osc_sequences() {
        let truncated = truncate_visible(
            "\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ tail",
            4,
        );

        assert!(truncated.contains("\x1b]8;;https://example.com\x1b\\"));
        assert!(truncated.contains("link"));
        assert!(truncated.contains("\x1b]8;;\x1b\\"));
        assert!(!truncated.contains("tail"));
    }
}
