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

pub fn status_label(status: Status) -> StyledObject<&'static str> {
    style_status(status_label_text(status), status)
}

pub fn status_cell(status: Status) -> StyledObject<String> {
    style_status(format!("{:<8}", status_label_text(status)), status)
}

pub fn status_line(status: Status, subject: impl fmt::Display, detail: impl fmt::Display) {
    println!("{}", status_line_text(status, subject, detail));
}

pub fn status_line_text(
    status: Status,
    subject: impl fmt::Display,
    detail: impl fmt::Display,
) -> String {
    format!(
        "{} {:<28} {}",
        status_cell(status),
        subject.to_string(),
        detail
    )
}

pub fn summary_line(status: Status, detail: impl fmt::Display) {
    println!("{} {}", status_cell(status), detail);
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
