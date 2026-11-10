use crate::storage::history::{HistoryRecord, LogLevel, Outcome};
use chrono::{DateTime, Duration, Local};
use std::fmt::Write as _;

pub fn day_label(record: &HistoryRecord) -> String {
    let Ok(timestamp) = DateTime::parse_from_rfc3339(&record.timestamp) else {
        return "Unknown date".to_string();
    };

    let date = timestamp.with_timezone(&Local).date_naive();
    let today = Local::now().date_naive();
    if date == today {
        "Today".to_string()
    } else if date == today - Duration::days(1) {
        "Yesterday".to_string()
    } else {
        date.format("%Y-%m-%d").to_string()
    }
}

pub fn local_time(record: &HistoryRecord) -> String {
    DateTime::parse_from_rfc3339(&record.timestamp)
        .map(|timestamp| timestamp.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|_| "--:--".to_string())
}

pub fn icon(record: &HistoryRecord) -> char {
    match record.outcome {
        Outcome::Cancelled => '−',
        Outcome::Failure => '✗',
        Outcome::Success if record.level == LogLevel::Warning => '!',
        Outcome::Success => '✓',
    }
}

pub fn render_records(records: &[HistoryRecord]) -> String {
    let action_width = records
        .iter()
        .map(|record| record.action.chars().count())
        .max()
        .unwrap_or(0)
        .max(8);

    let package_width = records
        .iter()
        .flat_map(|record| record.items.iter())
        .filter_map(|item| item.package.as_deref())
        .map(str::len)
        .max()
        .unwrap_or(0);

    let mut text = String::new();
    let mut previous_day = None;
    for record in records {
        let day = day_label(record);
        if previous_day.as_deref() != Some(day.as_str()) {
            if !text.is_empty() {
                text.push('\n');
            }

            writeln!(text, "{}", crate::output::section(&day))
                .expect("writing to a String cannot fail");
            previous_day = Some(day);
        }

        writeln!(
            text,
            "  {}  {} {:<action_width$}  {}",
            local_time(record),
            crate::output::status_marker(icon(record)),
            record.action,
            record.message
        )
        .expect("writing to a String cannot fail");

        let package_items = record
            .items
            .iter()
            .filter(|item| item.package.is_some())
            .collect::<Vec<_>>();

        let mut package_index = 0;
        for item in &record.items {
            let detail = item
                .from_version
                .as_ref()
                .zip(item.to_version.as_ref())
                .map(|(from, to)| format!("{from} → {to}"))
                .or_else(|| item.message.clone())
                .unwrap_or_default();

            if let Some(package) = &item.package {
                let branch = if package_index + 1 == package_items.len() {
                    '└'
                } else {
                    '├'
                };

                writeln!(
                    text,
                    "           {} {:<package_width$}  {detail}",
                    crate::output::meta(format!("{branch}─")),
                    package,
                )
                .expect("writing to a String cannot fail");
                package_index += 1;
            } else if !detail.is_empty() {
                writeln!(text, "           {detail}").expect("writing to a String cannot fail");
            }
        }
    }

    text.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::history::HistoryItem;

    fn record(action: &str, package: &str, outcome: Outcome) -> HistoryRecord {
        HistoryRecord {
            record_type: "operation".to_string(),
            operation_id: format!("{action}-{package}"),
            timestamp: "2026-01-01T04:26:00Z".to_string(),
            action: action.to_string(),
            level: if outcome == Outcome::Failure {
                LogLevel::Error
            } else {
                LogLevel::Info
            },
            outcome,
            message: "summary".to_string(),
            items: vec![HistoryItem {
                record_type: "item".to_string(),
                package: Some(package.to_string()),
                level: LogLevel::Info,
                outcome,
                message: Some("detail".to_string()),
                from_version: None,
                to_version: None,
            }],
        }
    }

    #[test]
    fn renders_parent_and_nested_item_rows() {
        let rendered = render_records(&[record("upgrade", "upstream", Outcome::Success)]);
        let output = console::strip_ansi_codes(&rendered);
        assert!(output.contains("✓ upgrade"));
        assert!(output.contains("└─ upstream"));
        assert!(output.contains("detail"));
    }
}
