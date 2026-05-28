use console::{StyledObject, style};
use indicatif::HumanBytes;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static ASSUME_YES: AtomicBool = AtomicBool::new(false);
use crate::services::packaging::disk_impact::{
    ByteEstimate, DiskImpact, SignedByteEstimate, SizeConfidence,
};

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

pub fn set_assume_yes(value: bool) {
    ASSUME_YES.store(value, Ordering::Relaxed);
}

pub fn assume_yes() -> bool {
    ASSUME_YES.load(Ordering::Relaxed)
}

pub fn confirm(prompt: impl fmt::Display) -> anyhow::Result<bool> {
    if assume_yes() {
        return Ok(true);
    }

    if !io::stdin().is_terminal() {
        anyhow::bail!(
            "Confirmation required for non-interactive input. Re-run with --yes to continue."
        );
    }

    print!("{} [y/N]: ", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub fn confirm_or_cancel(prompt: impl fmt::Display) -> anyhow::Result<()> {
    if confirm(prompt)? {
        return Ok(());
    }
    anyhow::bail!("Cancelled")
}

pub fn divider(width: usize) -> String {
    "-".repeat(width)
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

pub fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("api_token")
        || key.contains("token")
        || key.contains("secret")
        || key.contains("password")
}

pub fn redact_secret(value: &str) -> String {
    if value.is_empty() {
        return "(empty)".to_string();
    }
    if value.chars().count() <= 8 {
        return "********".to_string();
    }

    let prefix: String = value.chars().take(4).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
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

pub fn print_disk_impact(impact: &DiskImpact) {
    println!("{}", section("Disk impact:"));
    if !matches!(impact.download.bytes, Some(0)) {
        println!(
            "  {} {}",
            meta("Download:"),
            format_unsigned(impact.download)
        );
    }
    println!(
        "  {} {}",
        meta("Net disk change:"),
        format_signed(impact.net)
    );
}

pub fn print_local_disk_impact(impact: &DiskImpact) {
    println!("{}", section("Disk impact:"));
    println!("  {} {}", meta("Disk change:"), format_signed(impact.net));
}

fn format_unsigned(value: ByteEstimate) -> String {
    match value.bytes {
        Some(bytes) => format!(
            "{}{}",
            HumanBytes(bytes),
            confidence_suffix(value.confidence)
        ),
        None => "unknown".to_string(),
    }
}

fn format_signed(value: SignedByteEstimate) -> String {
    match value.bytes {
        Some(0) => format!("no change{}", confidence_suffix(value.confidence)),
        Some(bytes) if bytes > 0 => format!(
            "{} of additional disk space will be used{}",
            HumanBytes(bytes as u64),
            confidence_suffix(value.confidence)
        ),
        Some(bytes) => format!(
            "{} of disk space will be freed{}",
            HumanBytes(bytes.unsigned_abs() as u64),
            confidence_suffix(value.confidence)
        ),
        None => "unknown".to_string(),
    }
}

fn confidence_suffix(confidence: SizeConfidence) -> &'static str {
    match confidence {
        SizeConfidence::Exact => "",
        SizeConfidence::Estimated => " (estimated)",
        SizeConfidence::Unknown => "",
    }
}

#[cfg(test)]
mod tests {
    use console::strip_ansi_codes;

    use super::{
        Status, assume_yes, is_sensitive_key, redact_secret, set_assume_yes, status_cell,
        status_label, truncate_end, truncate_middle,
    };

    #[test]
    fn status_labels_are_stable_without_color() {
        assert_eq!(
            strip_ansi_codes(&status_label(Status::Ok).to_string()),
            "[ok]"
        );
        assert_eq!(
            strip_ansi_codes(&status_label(Status::Warn).to_string()),
            "[warn]"
        );
        assert_eq!(
            strip_ansi_codes(&status_label(Status::Fail).to_string()),
            "[fail]"
        );
        assert_eq!(
            strip_ansi_codes(&status_label(Status::Plan).to_string()),
            "[plan]"
        );
        assert_eq!(
            strip_ansi_codes(&status_label(Status::Skip).to_string()),
            "[skip]"
        );
    }

    #[test]
    fn status_cells_are_padded_before_styling() {
        assert_eq!(
            strip_ansi_codes(&status_cell(Status::Ok).to_string()),
            "[ok]    "
        );
        assert_eq!(
            strip_ansi_codes(&status_cell(Status::Plan).to_string()),
            "[plan]  "
        );
    }

    #[test]
    fn truncation_helpers_are_stable() {
        assert_eq!(truncate_end("abcdefghijklmnopqrstuvwxyz", 10), "abcdefg...");
        assert_eq!(
            truncate_middle("abcdefghijklmnopqrstuvwxyz", 10),
            "abc...wxyz"
        );
        assert_eq!(truncate_end("abc", 10), "abc");
        assert_eq!(truncate_middle("abc", 10), "abc");
    }

    #[test]
    fn sensitive_values_are_detected_and_redacted() {
        assert!(is_sensitive_key("github.api_token"));
        assert!(is_sensitive_key("auth.password"));
        assert!(!is_sensitive_key("github.enabled"));
        assert_eq!(
            redact_secret("ghp_abcdefghijklmnopqrstuvwxyz"),
            "ghp_...wxyz"
        );
        assert_eq!(redact_secret("short"), "********");
    }

    #[test]
    fn assume_yes_flag_is_shared() {
        set_assume_yes(false);
        assert!(!assume_yes());
        set_assume_yes(true);
        assert!(assume_yes());
        set_assume_yes(false);
    }
}
