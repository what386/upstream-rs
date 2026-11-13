//! Human-readable formatting for elapsed time and byte counts.

use chrono::{DateTime, Utc};
use indicatif::HumanBytes;

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

fn elapsed_seconds(checked_at: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - checked_at).num_seconds().max(0)
}

/// Compact elapsed age, such as `40m ago`. Future timestamps become `0s ago`.
pub fn compact_age(checked_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = elapsed_seconds(checked_at, now);
    let (amount, unit) = if seconds >= SECONDS_PER_DAY {
        (seconds / SECONDS_PER_DAY, "d")
    } else if seconds >= SECONDS_PER_HOUR {
        (seconds / SECONDS_PER_HOUR, "h")
    } else if seconds >= SECONDS_PER_MINUTE {
        (seconds / SECONDS_PER_MINUTE, "m")
    } else {
        (seconds, "s")
    };
    format!("{amount}{unit} ago")
}

/// Elapsed age in words, such as `2 hours ago`, or `just now` below one minute.
pub fn age(checked_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let seconds = elapsed_seconds(checked_at, now);
    let (amount, unit) = if seconds >= SECONDS_PER_DAY {
        (seconds / SECONDS_PER_DAY, "day")
    } else if seconds >= SECONDS_PER_HOUR {
        (seconds / SECONDS_PER_HOUR, "hour")
    } else if seconds >= SECONDS_PER_MINUTE {
        (seconds / SECONDS_PER_MINUTE, "minute")
    } else {
        return "just now".into();
    };
    format!("{amount} {unit}{} ago", if amount == 1 { "" } else { "s" })
}

/// Coarse date age for repository listings. Months use 30 days and years 12 months.
pub fn relative_date(updated_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let days = elapsed_seconds(updated_at, now) / SECONDS_PER_DAY;
    if days == 0 {
        return "today".into();
    }
    let (amount, unit) = if days < 30 {
        (days, "day")
    } else if days / 30 < 12 {
        (days / 30, "month")
    } else {
        (days / 30 / 12, "year")
    };
    format!("{amount} {unit}{} ago", if amount == 1 { "" } else { "s" })
}

/// Byte-transfer progress, omitting an unknown total and using `-` when both are zero.
pub fn format_transfer(completed: u64, total: u64) -> String {
    if total > 0 {
        format!("{} / {}", HumanBytes(completed), HumanBytes(total))
    } else if completed > 0 {
        HumanBytes(completed).to_string()
    } else {
        "-".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[test]
    fn compact_ages_use_seconds_minutes_hours_and_days() {
        let now = Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap();
        for (seconds, expected) in [
            (-60, "0s ago"),
            (0, "0s ago"),
            (59, "59s ago"),
            (60, "1m ago"),
            (2_400, "40m ago"),
            (3_599, "59m ago"),
            (3_600, "1h ago"),
            (86_399, "23h ago"),
            (86_400, "1d ago"),
            (30 * 86_400, "30d ago"),
        ] {
            assert_eq!(compact_age(now - Duration::seconds(seconds), now), expected);
        }
    }

    #[test]
    fn ages_in_words_handle_singular_plural_and_future_timestamps() {
        let now = Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap();
        for (seconds, expected) in [
            (-3_600, "just now"),
            (59, "just now"),
            (60, "1 minute ago"),
            (120, "2 minutes ago"),
            (3_600, "1 hour ago"),
            (7_200, "2 hours ago"),
            (86_400, "1 day ago"),
            (172_800, "2 days ago"),
        ] {
            assert_eq!(age(now - Duration::seconds(seconds), now), expected);
        }
    }

    #[test]
    fn relative_dates_use_readable_buckets() {
        let now = Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap();
        for (days, expected) in [
            (-1, "today"),
            (0, "today"),
            (1, "1 day ago"),
            (2, "2 days ago"),
            (30, "1 month ago"),
            (65, "2 months ago"),
            (360, "1 year ago"),
            (800, "2 years ago"),
        ] {
            assert_eq!(relative_date(now - Duration::days(days), now), expected);
        }
    }

    #[test]
    fn byte_transfers_handle_known_and_unknown_totals() {
        assert_eq!(format_transfer(0, 0), "-");
        assert_eq!(format_transfer(1_024, 0), "1.00 KiB");
        assert_eq!(format_transfer(1_024, 2_048), "1.00 KiB / 2.00 KiB");
        assert_eq!(format_transfer(0, 1_024), "0 B / 1.00 KiB");
    }
}
