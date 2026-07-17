use console::{StyledObject, style};
use std::{collections::HashSet, fmt};

const STATUS_CELL_WIDTH: usize = 7;
const STATUS_SUBJECT_MARGIN: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Plan,
    Skip,
}

pub fn status_label(status: Status) -> StyledObject<&'static str> {
    style_status(status_label_text(status), status)
}

pub fn status_cell(status: Status) -> StyledObject<String> {
    style_status(
        format!("{:<STATUS_CELL_WIDTH$}", status_label_text(status)),
        status,
    )
}

pub fn status_line(status: Status, subject: impl fmt::Display, detail: impl fmt::Display) {
    println!("{}", status_line_text(status, subject, detail));
}

pub fn status_line_text(
    status: Status,
    subject: impl fmt::Display,
    detail: impl fmt::Display,
) -> String {
    let subject = subject.to_string();
    let detail = detail.to_string();
    status_line_text_with_width(
        status,
        &subject,
        detail,
        status_subject_width([subject.as_str()]),
    )
}

pub fn status_line_text_with_width(
    status: Status,
    subject: impl fmt::Display,
    detail: impl fmt::Display,
    subject_width: usize,
) -> String {
    let subject = subject.to_string();
    let detail = detail.to_string();
    if let Some(status_name) = log_status_name(status) {
        super::logger::status(subject.clone(), status_name, detail.clone());
    }
    format!(
        "{} {:<subject_width$} {}",
        status_cell(status),
        subject,
        detail
    )
}

fn log_status_name(status: Status) -> Option<&'static str> {
    match status {
        Status::Ok => Some("ok"),
        Status::Warn => Some("warn"),
        Status::Fail => Some("fail"),
        Status::Plan | Status::Skip => None,
    }
}

pub fn status_subject_width<'a>(subjects: impl IntoIterator<Item = &'a str>) -> usize {
    subjects
        .into_iter()
        .map(|subject| subject.chars().count())
        .max()
        .unwrap_or(0)
        + STATUS_SUBJECT_MARGIN
}

pub fn summary_line(status: Status, detail: impl fmt::Display) {
    println!("{} {}", status_cell(status), detail);
}

const ERROR_SUMMARY_MAX_CHARS: usize = 160;

pub fn error_summary(err: &anyhow::Error) -> String {
    error_summary_with_limit(err, ERROR_SUMMARY_MAX_CHARS)
}

pub fn error_summary_with_limit(err: &anyhow::Error, max: usize) -> String {
    let mut seen = HashSet::new();
    let mut parts = Vec::new();
    for message in err.chain().map(std::string::ToString::to_string) {
        if seen.insert(message.clone()) {
            parts.push(message);
        }
    }

    let Some(root) = parts.last() else {
        return truncate_for_error(&err.to_string(), max);
    };
    let Some(parent) = parts.iter().rev().nth(1) else {
        return truncate_for_error(root, max);
    };

    let value = format!("{parent}: {root}");
    if value.chars().count() <= max {
        return value;
    }

    let root_len = root.chars().count();
    if root_len.saturating_add(2) >= max {
        return truncate_for_error(root, max);
    }

    let parent_max = max - root_len - 2;
    format!("{}: {}", truncate_for_error(parent, parent_max), root)
}

fn truncate_for_error(value: &str, max: usize) -> String {
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

fn status_label_text(status: Status) -> &'static str {
    match status {
        Status::Ok => "[ok]",
        Status::Warn => "[warn]",
        Status::Fail => "[fail]",
        Status::Plan => "[plan]",
        Status::Skip => "[skip]",
    }
}

fn style_status<T: fmt::Display>(text: T, status: Status) -> StyledObject<T> {
    match status {
        Status::Ok => style(text).green(),
        Status::Warn => style(text).yellow(),
        Status::Fail => style(text).red(),
        Status::Plan => style(text).yellow(),
        Status::Skip => style(text).dim(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Status, error_summary, status_line_text, status_line_text_with_width, status_subject_width,
    };

    #[test]
    fn error_summary_returns_single_layer_error() {
        let err = anyhow::anyhow!("plain failure");

        assert_eq!(error_summary(&err), "plain failure");
    }

    #[test]
    fn status_line_uses_compact_subject_spacing() {
        assert_eq!(
            console::strip_ansi_codes(&status_line_text(Status::Ok, "gh", "upgraded to 2.94.0"))
                .to_string(),
            "[ok]    gh    upgraded to 2.94.0"
        );
    }

    #[test]
    fn status_line_can_align_to_batch_subject_width() {
        let width = status_subject_width(["gh", "ripgrep"]);

        assert_eq!(
            console::strip_ansi_codes(&status_line_text_with_width(
                Status::Ok,
                "gh",
                "upgraded",
                width
            ))
            .to_string(),
            "[ok]    gh         upgraded"
        );
        assert_eq!(
            console::strip_ansi_codes(&status_line_text_with_width(
                Status::Ok,
                "ripgrep",
                "upgraded",
                width
            ))
            .to_string(),
            "[ok]    ripgrep    upgraded"
        );
    }

    #[test]
    fn error_summary_returns_parent_and_root_cause() {
        let err = anyhow::anyhow!("os error 5")
            .context("file does not exist")
            .context("Failed to install archive");

        assert_eq!(error_summary(&err), "file does not exist: os error 5");
    }

    #[test]
    fn error_summary_omits_outer_wrappers() {
        let err = anyhow::anyhow!("Access is denied. (os error 5)")
            .context("Failed to move extracted directory")
            .context("Failed to install archive")
            .context("Failed to perform installation for 'just'");

        assert_eq!(
            error_summary(&err),
            "Failed to move extracted directory: Access is denied. (os error 5)"
        );
    }

    #[test]
    fn error_summary_truncates_parent_before_root() {
        let err = anyhow::anyhow!("Access is denied. (os error 5)")
            .context("Failed to move extracted directory from a very long source path")
            .context("Failed to install archive");

        let formatted = super::error_summary_with_limit(&err, 52);

        assert!(formatted.ends_with(": Access is denied. (os error 5)"));
        assert!(formatted.starts_with("Failed to move"));
        assert!(formatted.contains("...: Access is denied"));
        assert!(formatted.chars().count() <= 52);
    }

    #[test]
    fn error_summary_truncates_root_when_root_is_too_long() {
        let err = anyhow::anyhow!("this root cause is too long to fit").context("context");

        let formatted = super::error_summary_with_limit(&err, 12);

        assert_eq!(formatted, "this root...");
    }
}
