use chrono::{DateTime, Duration, Local, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fmt::Write as _,
    process,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Success,
    Failure,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub record_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    pub level: LogLevel,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub record_type: String,
    pub operation_id: String,
    pub timestamp: String,
    pub action: String,
    pub level: LogLevel,
    pub outcome: Outcome,
    pub message: String,
    #[serde(default)]
    pub items: Vec<HistoryItem>,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub package: Option<String>,
    pub action: Option<String>,
    pub status: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub today: bool,
    pub limit: usize,
}

struct ActiveOperation {
    record: HistoryRecord,
    mutating: bool,
    warning: bool,
}

static ACTIVE: OnceLock<Mutex<Option<ActiveOperation>>> = OnceLock::new();
static COUNTER: OnceLock<Mutex<u64>> = OnceLock::new();

fn active() -> &'static Mutex<Option<ActiveOperation>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}

pub fn begin(action: impl Into<String>, mutating: bool) {
    let timestamp = Utc::now().to_rfc3339();
    let operation_id = operation_id();
    let operation = ActiveOperation {
        record: HistoryRecord {
            record_type: "operation".to_string(),
            operation_id,
            timestamp,
            action: action.into(),
            level: LogLevel::Info,
            outcome: Outcome::Success,
            message: String::new(),
            items: Vec::new(),
        },
        mutating,
        warning: false,
    };
    if let Ok(mut current) = active().lock() {
        *current = Some(operation);
    }
}

pub fn record_item(
    package: Option<String>,
    level: LogLevel,
    outcome: Outcome,
    message: impl Into<String>,
) {
    let Ok(mut current) = active().lock() else {
        return;
    };
    let Some(operation) = current.as_mut() else {
        return;
    };
    if level == LogLevel::Warning {
        operation.warning = true;
    }
    operation.record.items.push(HistoryItem {
        record_type: "item".to_string(),
        package,
        level,
        outcome,
        message: Some(message.into()),
        from_version: None,
        to_version: None,
    });
}

pub fn record_version_item(
    package: impl Into<String>,
    from_version: impl Into<String>,
    to_version: impl Into<String>,
) {
    let Ok(mut current) = active().lock() else {
        return;
    };
    let Some(operation) = current.as_mut() else {
        return;
    };
    let package = package.into();
    if let Some(item) = operation
        .record
        .items
        .iter_mut()
        .rev()
        .find(|item| item.package.as_deref() == Some(package.as_str()))
    {
        item.message = None;
        item.from_version = Some(from_version.into());
        item.to_version = Some(to_version.into());
    } else {
        operation.record.items.push(HistoryItem {
            record_type: "item".to_string(),
            package: Some(package),
            level: LogLevel::Info,
            outcome: Outcome::Success,
            message: None,
            from_version: Some(from_version.into()),
            to_version: Some(to_version.into()),
        });
    }
}

pub fn finish(outcome: Outcome, level: LogLevel, message: Option<String>) {
    let operation = active().lock().ok().and_then(|mut current| current.take());
    let Some(mut operation) = operation else {
        return;
    };

    operation.record.outcome = outcome;
    operation.record.level = if operation.warning && level == LogLevel::Info {
        LogLevel::Warning
    } else {
        level
    };
    operation.record.message = message
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| summarize(&operation.record));

    let is_read_only_success = !operation.mutating
        && outcome == Outcome::Success
        && operation.record.level == LogLevel::Info;
    let is_empty_upgrade = operation.record.action == "upgrade"
        && outcome == Outcome::Success
        && operation.record.items.is_empty();
    let is_history_success = operation.record.action == "history" && outcome == Outcome::Success;
    if !is_read_only_success && !is_empty_upgrade && !is_history_success {
        crate::output::write_operation(operation.record);
    }
}

fn summarize(record: &HistoryRecord) -> String {
    let successful = record
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Success)
        .count();
    let failed = record
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Failure)
        .count();
    let cancelled = record
        .items
        .iter()
        .filter(|item| item.outcome == Outcome::Cancelled)
        .count();

    match record.outcome {
        Outcome::Cancelled => {
            if successful > 0 {
                format!("interrupted after completing {successful} item(s)")
            } else {
                "interrupted".to_string()
            }
        }
        Outcome::Failure if failed > 0 => {
            format!("failed after completing {successful} item(s)")
        }
        Outcome::Failure => "failed".to_string(),
        Outcome::Success if successful > 0 => {
            let verb = match record.action.as_str() {
                "upgrade" => "upgraded",
                "import" => "imported",
                "remove" => "removed",
                "rollback" => "restored",
                "reinstall" => "reinstalled",
                "install" | "build" | "find" | "probe" => "installed",
                _ => "completed",
            };
            if failed > 0 {
                format!("{successful} {verb}, {failed} failed")
            } else if cancelled > 0 {
                format!("{successful} {verb}, {cancelled} cancelled")
            } else {
                format!("{successful} {verb}")
            }
        }
        Outcome::Success => "completed".to_string(),
    }
}

pub fn parse_since(raw: &str) -> Result<DateTime<Utc>, String> {
    let raw = raw.trim();
    if raw.len() < 2 {
        return Err("expected a duration such as 2d, 12h, or 30m".to_string());
    }
    let (number, unit) = raw.split_at(raw.len() - 1);
    let amount: i64 = number
        .parse()
        .map_err(|_| format!("invalid duration '{raw}'"))?;
    if amount < 0 {
        return Err("duration cannot be negative".to_string());
    }
    let duration = match unit {
        "m" => Duration::minutes(amount),
        "h" => Duration::hours(amount),
        "d" => Duration::days(amount),
        "w" => Duration::weeks(amount),
        _ => return Err("expected a duration such as 2d, 12h, or 30m".to_string()),
    };
    Ok(Utc::now() - duration)
}

pub fn filter_records(
    mut records: Vec<HistoryRecord>,
    filter: &HistoryFilter,
) -> Vec<HistoryRecord> {
    let today = Local::now().date_naive();
    records.retain(|record| {
        let timestamp = DateTime::parse_from_rfc3339(&record.timestamp)
            .map(|timestamp| timestamp.with_timezone(&Utc));
        let package_matches = filter.package.as_ref().is_none_or(|package| {
            record.items.iter().any(|item| {
                item.package
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(package))
            })
        });
        let action_matches = filter
            .action
            .as_ref()
            .is_none_or(|action| record.action.starts_with(action));
        let status_matches = filter.status.as_ref().is_none_or(|status| {
            let status = normalize_status(status);
            status == outcome_name(record.outcome) || status == level_name(record.level)
        });
        let since_matches = filter
            .since
            .is_none_or(|since| timestamp.is_ok_and(|value| value >= since));
        let today_matches = !filter.today
            || timestamp.is_ok_and(|value| value.with_timezone(&Local).date_naive() == today);
        package_matches && action_matches && status_matches && since_matches && today_matches
    });
    records.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    if filter.limit > 0 {
        records.truncate(filter.limit);
    }
    records
}

fn normalize_status(status: &str) -> String {
    match status.to_ascii_lowercase().as_str() {
        "failed" | "fail" => "failure".to_string(),
        "cancel" | "canceled" => "cancelled".to_string(),
        "warn" => "warning".to_string(),
        value => value.to_string(),
    }
}

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Success => "success",
        Outcome::Failure => "failure",
        Outcome::Cancelled => "cancelled",
    }
}

fn level_name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Info => "info",
        LogLevel::Warning => "warning",
        LogLevel::Error => "error",
    }
}

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

fn operation_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = COUNTER.get_or_init(|| Mutex::new(0));
    let value = counter
        .lock()
        .map(|mut counter| {
            *counter = counter.wrapping_add(1);
            *counter
        })
        .unwrap_or_default();
    format!("op-{nanos}-{}-{value}", process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn filters_operations_by_nested_package_and_outcome() {
        let records = vec![
            record("upgrade", "upstream", Outcome::Success),
            record("find", "cosign", Outcome::Failure),
        ];
        let filtered = filter_records(
            records,
            &HistoryFilter {
                package: Some("upstream".to_string()),
                status: Some("success".to_string()),
                limit: 20,
                ..HistoryFilter::default()
            },
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].action, "upgrade");
    }

    #[test]
    fn renders_parent_and_nested_item_rows() {
        let rendered = render_records(&[record("upgrade", "upstream", Outcome::Success)]);
        let output = console::strip_ansi_codes(&rendered);
        assert!(output.contains("✓ upgrade"));
        assert!(output.contains("└─ upstream"));
        assert!(output.contains("detail"));
    }

    #[test]
    fn serializes_nested_items_in_one_operation_record() {
        let value: serde_json::Value =
            serde_json::to_value(record("upgrade", "upstream", Outcome::Success))
                .expect("serialize history record");
        assert_eq!(value["record_type"], "operation");
        assert_eq!(value["items"][0]["record_type"], "item");
        assert_eq!(value["items"][0]["package"], "upstream");
    }
}
