use indicatif::HumanBytes;

use crate::output::{divider, meta, section, truncate_end};
use crate::services::packaging::disk_impact::{
    ByteEstimate, DiskImpact, SignedByteEstimate, SizeConfidence,
};

pub struct TransactionRow {
    pub package: String,
    pub old_version: String,
    pub new_version: Option<String>,
    pub net_change: SignedByteEstimate,
    pub download: ByteEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeImpactRow {
    pub label: String,
    pub value: SignedByteEstimate,
}

impl SizeImpactRow {
    pub fn new(label: impl Into<String>, value: SignedByteEstimate) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

impl TransactionRow {
    pub fn new(
        package: impl Into<String>,
        old_version: impl Into<String>,
        new_version: impl Into<String>,
        net_change: SignedByteEstimate,
        download: ByteEstimate,
    ) -> Self {
        Self {
            package: package.into(),
            old_version: old_version.into(),
            new_version: Some(new_version.into()),
            net_change,
            download,
        }
    }

    pub fn single_version(
        package: impl Into<String>,
        version: impl Into<String>,
        net_change: SignedByteEstimate,
        download: ByteEstimate,
    ) -> Self {
        Self {
            package: package.into(),
            old_version: version.into(),
            new_version: None,
            net_change,
            download,
        }
    }
}

pub fn print_transaction_table(rows: &[TransactionRow], totals: &DiskImpact, net_label: &str) {
    print_transaction_table_with_size_rows(rows, totals, net_label, &[]);
}

pub fn print_transaction_table_without_size(rows: &[TransactionRow]) {
    let layout = TransactionTableLayout::from_rows_without_size(rows);
    layout.print_header();
    for row in rows {
        layout.print_row(row);
    }
    println!();
}

pub fn print_transaction_table_with_size_rows(
    rows: &[TransactionRow],
    totals: &DiskImpact,
    net_label: &str,
    size_rows: &[SizeImpactRow],
) {
    let layout = TransactionTableLayout::from_rows(rows);
    layout.print_header();
    for row in rows {
        layout.print_row(row);
    }
    layout.print_totals(totals, net_label, size_rows);
}

pub struct TransactionTableLayout {
    package_label: String,
    package_width: usize,
    show_download: bool,
    show_new_version: bool,
    show_net_change: bool,
    net_magnitude_width: usize,
}

const LIVE_UPGRADE_NET_MAGNITUDE_WIDTH: usize = 10;

impl TransactionTableLayout {
    pub fn from_rows(rows: &[TransactionRow]) -> Self {
        let package_header = format!("Package ({})", rows.len());
        let package_width = rows
            .iter()
            .map(|row| row.package.chars().count())
            .chain(std::iter::once(package_header.chars().count()))
            .max()
            .unwrap_or(package_header.len())
            .clamp(11, 44);
        let show_download = rows.iter().any(|row| row.download.bytes != Some(0));
        let show_new_version = rows.iter().any(|row| row.new_version.is_some());
        let net_magnitude_width = rows
            .iter()
            .map(|row| compact_signed_magnitude(row.net_change).chars().count())
            .chain(std::iter::once("Net Change".len().saturating_sub(1)))
            .max()
            .unwrap_or(9);

        Self {
            package_label: format!("Package ({})", rows.len()),
            package_width,
            show_download,
            show_new_version,
            show_net_change: true,
            net_magnitude_width,
        }
    }

    pub fn from_rows_without_size(rows: &[TransactionRow]) -> Self {
        let mut layout = Self::from_rows(rows);
        layout.show_download = false;
        layout.show_net_change = false;
        layout
    }

    pub fn upgrade_preview(package_width: usize) -> Self {
        Self {
            package_label: "Package".to_string(),
            package_width: package_width.max("Package".len()).min(44),
            show_download: true,
            show_new_version: true,
            show_net_change: true,
            net_magnitude_width: LIVE_UPGRADE_NET_MAGNITUDE_WIDTH,
        }
    }

    fn header_line(&self) -> String {
        let version_header = if self.show_new_version {
            "Old Version"
        } else {
            "Version"
        };
        let net_width = self.net_magnitude_width + 1;

        let mut line = format!(
            "{:<package_width$} {:<12}",
            self.package_label,
            version_header,
            package_width = self.package_width
        );
        if self.show_new_version {
            line.push_str(&format!(" {:<13}", "New Version"));
        }
        if self.show_net_change {
            line.push_str(&format!(" {:>net_width$}", "Net Change"));
        }
        if self.show_download {
            line.push_str(&format!(" {:>14}", "Download Size"));
        }
        line
    }

    fn divider_line(&self) -> String {
        divider(self.header_line().len())
    }

    pub fn print_header(&self) {
        println!("{}", self.header_line());
        println!("{}", self.divider_line());
    }

    fn row_line(&self, row: &TransactionRow) -> String {
        let mut line = format!(
            "{:<package_width$} {:<12}",
            truncate_end(&row.package, self.package_width),
            truncate_end(&row.old_version, 12),
            package_width = self.package_width
        );
        if self.show_new_version {
            line.push_str(&format!(
                " {:<13}",
                truncate_end(row.new_version.as_deref().unwrap_or("-"), 13)
            ));
        }
        if self.show_net_change {
            line.push_str(&format!(
                " {}",
                format_compact_signed_cell(row.net_change, self.net_magnitude_width)
            ));
        }
        if self.show_download {
            line.push_str(&format!(" {:>14}", format_compact_unsigned(row.download)));
        }
        line
    }

    pub fn print_row(&self, row: &TransactionRow) {
        print!("{}", self.row_line(row));
        println!();
    }

    pub fn print_totals(&self, totals: &DiskImpact, net_label: &str, size_rows: &[SizeImpactRow]) {
        println!();
        if self.show_download && !matches!(totals.download.bytes, Some(0)) {
            println!(
                "Total Download Size:   {}",
                format_compact_unsigned(totals.download)
            );
        }
        if size_rows.is_empty() {
            println!("{net_label:<22} {}", format_compact_signed(totals.net));
        } else {
            println!(
                "{:<22} {}",
                "Package files:",
                format_compact_delta(totals.net)
            );
            for row in size_rows {
                println!(
                    "{:<22} {}",
                    format!("{}:", row.label),
                    format_compact_delta(row.value)
                );
            }
            println!(
                "{:<22} {}",
                "Net disk change:",
                format_compact_signed(total_disk_change(totals.net, size_rows))
            );
        }
        println!();
    }
}

pub fn print_disk_impact(impact: &DiskImpact, include_download: bool) {
    print_disk_impact_with_size_rows(impact, &[], include_download);
}

pub fn print_disk_impact_with_size_rows(
    impact: &DiskImpact,
    size_rows: &[SizeImpactRow],
    include_download: bool,
) {
    println!("{}", section("Size impact:"));
    if include_download && !matches!(impact.download.bytes, Some(0)) {
        println!(
            "  {} {}",
            meta("Download:"),
            format_unsigned(impact.download)
        );
    }
    if size_rows.is_empty() {
        println!(
            "  {} {}",
            meta("Net disk change:"),
            format_signed(impact.net)
        );
        return;
    }

    println!(
        "  {} {}",
        meta("Package files:"),
        format_signed_delta(impact.net)
    );
    for row in size_rows {
        println!(
            "  {} {}",
            meta(format!("{}:", row.label)),
            format_signed_delta(row.value)
        );
    }
    println!(
        "  {} {}",
        meta("Net disk change:"),
        format_signed(total_disk_change(impact.net, size_rows))
    );
}

fn total_disk_change(
    package_files: SignedByteEstimate,
    size_rows: &[SizeImpactRow],
) -> SignedByteEstimate {
    size_rows
        .iter()
        .fold(package_files, |total, row| total + row.value)
}

fn format_compact_unsigned(value: ByteEstimate) -> String {
    match value.bytes {
        Some(bytes) => format!("{}", HumanBytes(bytes)),
        None => "unknown".to_string(),
    }
}

fn format_compact_signed(value: SignedByteEstimate) -> String {
    match value.bytes {
        Some(bytes) => {
            let magnitude = HumanBytes(bytes.unsigned_abs() as u64);
            if bytes < 0 {
                format!("-{magnitude}")
            } else {
                format!("{magnitude}")
            }
        }
        None => "unknown".to_string(),
    }
}

fn format_compact_delta(value: SignedByteEstimate) -> String {
    match value.bytes {
        Some(bytes) if bytes > 0 => format!("+{}", HumanBytes(bytes as u64)),
        Some(bytes) if bytes < 0 => format!("-{}", HumanBytes(bytes.unsigned_abs() as u64)),
        Some(_) => "no change".to_string(),
        None => "unknown".to_string(),
    }
}

fn format_compact_signed_cell(value: SignedByteEstimate, magnitude_width: usize) -> String {
    match value.bytes {
        Some(bytes) => {
            let sign = if bytes < 0 { "-" } else { " " };
            let magnitude = compact_signed_magnitude(value);
            format!("{sign}{magnitude:<magnitude_width$}")
        }
        None => format!(" {:<magnitude_width$}", "unknown"),
    }
}

fn compact_signed_magnitude(value: SignedByteEstimate) -> String {
    match value.bytes {
        Some(bytes) => {
            let magnitude = HumanBytes(bytes.unsigned_abs() as u64);
            format!("{magnitude}")
        }
        None => "unknown".to_string(),
    }
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
        Some(bytes) if bytes > 0 => {
            format!(
                "{}{}",
                HumanBytes(bytes as u64),
                confidence_suffix(value.confidence)
            )
        }
        Some(bytes) => format!(
            "-{}{}",
            HumanBytes(bytes.unsigned_abs() as u64),
            confidence_suffix(value.confidence)
        ),
        None => "unknown".to_string(),
    }
}

fn format_signed_delta(value: SignedByteEstimate) -> String {
    match value.bytes {
        Some(bytes) if bytes > 0 => format!(
            "+{}{}",
            HumanBytes(bytes as u64),
            confidence_suffix(value.confidence)
        ),
        Some(bytes) if bytes < 0 => format!(
            "-{}{}",
            HumanBytes(bytes.unsigned_abs() as u64),
            confidence_suffix(value.confidence)
        ),
        Some(_) => format!("no change{}", confidence_suffix(value.confidence)),
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
    use crate::services::packaging::disk_impact::{ByteEstimate, SignedByteEstimate};

    use super::{
        SizeImpactRow, TransactionRow, TransactionTableLayout, format_compact_delta, format_signed,
        format_signed_delta, total_disk_change,
    };

    #[test]
    fn live_upgrade_preview_keeps_download_column_aligned() {
        let layout = TransactionTableLayout::upgrade_preview("stable/forge".len());
        let row = TransactionRow::new(
            "stable/forge",
            "0.1.2",
            "0.2.2",
            SignedByteEstimate::estimated(-227_604),
            ByteEstimate::exact(5 * 1024 * 1024),
        );

        let header = layout.header_line();
        let rendered_row = layout.row_line(&row);

        assert_eq!(header.len(), rendered_row.len());
        assert_eq!(
            header.find("Download Size").expect("download header") + "Download Size".len(),
            rendered_row.find("5.00 MiB").expect("download size") + "5.00 MiB".len()
        );
        assert_eq!(layout.divider_line(), "-".repeat(header.len()));
    }

    #[test]
    fn live_upgrade_preview_uses_computed_package_width() {
        let layout = TransactionTableLayout::upgrade_preview("stable/gh".len());

        assert_eq!(layout.package_width, "stable/gh".len());
        assert!(layout.header_line().starts_with("Package   Old Version"));
    }

    #[test]
    fn signed_disk_impact_uses_label_context() {
        assert_eq!(
            format_signed(SignedByteEstimate::estimated(5 * 1024 * 1024)),
            "5.00 MiB (estimated)"
        );
        assert_eq!(
            format_signed(SignedByteEstimate::exact(-5 * 1024 * 1024)),
            "-5.00 MiB"
        );
    }

    #[test]
    fn auxiliary_size_rows_render_as_deltas() {
        assert_eq!(
            format_signed_delta(SignedByteEstimate::exact(5 * 1024 * 1024)),
            "+5.00 MiB"
        );
        assert_eq!(
            format_signed_delta(SignedByteEstimate::estimated(-5 * 1024 * 1024)),
            "-5.00 MiB (estimated)"
        );
        assert_eq!(
            format_compact_delta(SignedByteEstimate::exact(5 * 1024 * 1024)),
            "+5.00 MiB"
        );
    }

    #[test]
    fn total_disk_change_includes_auxiliary_rows() {
        let total = total_disk_change(
            SignedByteEstimate::exact(-10),
            &[SizeImpactRow::new(
                "Rollback storage",
                SignedByteEstimate::exact(10),
            )],
        );

        assert_eq!(total.bytes, Some(0));
    }
}
