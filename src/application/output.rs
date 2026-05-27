use console::{StyledObject, style};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Plan,
    Skip,
}

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

pub fn failure(text: impl fmt::Display) -> StyledObject<String> {
    style(text.to_string()).red()
}

pub fn kv(label: &str, value: impl fmt::Display) {
    println!("  {:<13} {}", meta(format!("{label}:")), value);
}

pub fn action_note(text: impl fmt::Display) {
    println!("  {}", meta(text));
}

pub fn status_label(status: Status) -> StyledObject<&'static str> {
    match status {
        Status::Ok => style("[ok]").green(),
        Status::Warn => style("[warn]").yellow(),
        Status::Fail => style("[fail]").red(),
        Status::Plan => style("[plan]").yellow(),
        Status::Skip => style("[skip]").dim(),
    }
}

pub fn status_cell(status: Status) -> StyledObject<String> {
    let label = match status {
        Status::Ok => "[ok]",
        Status::Warn => "[warn]",
        Status::Fail => "[fail]",
        Status::Plan => "[plan]",
        Status::Skip => "[skip]",
    };
    let padded = format!("{label:<8}");
    match status {
        Status::Ok => style(padded).green(),
        Status::Warn => style(padded).yellow(),
        Status::Fail => style(padded).red(),
        Status::Plan => style(padded).yellow(),
        Status::Skip => style(padded).dim(),
    }
}

pub fn status_line(status: Status, subject: impl fmt::Display, detail: impl fmt::Display) {
    println!(
        "{} {:<28} {}",
        status_cell(status),
        subject.to_string(),
        detail
    );
}

pub fn summary_line(status: Status, detail: impl fmt::Display) {
    println!("{} {}", status_cell(status), detail);
}

#[cfg(test)]
mod tests {
    use super::{Status, status_cell, status_label};

    #[test]
    fn status_labels_are_stable_without_color() {
        assert_eq!(status_label(Status::Ok).to_string(), "[ok]");
        assert_eq!(status_label(Status::Warn).to_string(), "[warn]");
        assert_eq!(status_label(Status::Fail).to_string(), "[fail]");
        assert_eq!(status_label(Status::Plan).to_string(), "[plan]");
        assert_eq!(status_label(Status::Skip).to_string(), "[skip]");
    }

    #[test]
    fn status_cells_are_padded_before_styling() {
        assert_eq!(status_cell(Status::Ok).to_string(), "[ok]    ");
        assert_eq!(status_cell(Status::Plan).to_string(), "[plan]  ");
    }
}
