use console::{Alignment, measure_text_width, pad_str, truncate_str};

use crate::models::provider::SearchMetadataValue;

pub struct SearchTableRow {
    pub source: String,
    pub metadata: Vec<(String, SearchMetadataValue)>,
    pub updated: String,
    pub description: String,
}

pub struct SearchTable {
    pub header: String,
    pub divider: String,
    pub rows: Vec<String>,
}

impl SearchTable {
    pub fn new(source_heading: &str, rows: &[SearchTableRow], terminal_width: usize) -> Self {
        let terminal_width = terminal_width.max(1);
        let mut keys = Vec::new();
        for row in rows {
            for (key, _) in &row.metadata {
                if !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
        }

        let mut headings = vec![source_heading.to_owned()];
        headings.extend(keys.iter().map(|key| heading(key)));
        headings.extend(["Updated".into(), "Description".into()]);
        let cells = rows
            .iter()
            .map(|row| {
                let mut cells = vec![(clean(&row.source), false)];
                for key in &keys {
                    let cell = match row
                        .metadata
                        .iter()
                        .find(|(k, _)| k == key)
                        .map(|(_, value)| value)
                    {
                        Some(SearchMetadataValue::Integer(value)) => (format_count(*value), true),
                        Some(SearchMetadataValue::Text(value)) => (clean(value), false),
                        None => ("-".into(), false),
                    };

                    cells.push(cell);
                }

                cells.push((clean(&row.updated), false));
                cells.push((clean(&row.description), false));
                cells
            })
            .collect::<Vec<_>>();

        let prefix_columns = headings.len() - 1;
        let mut widths = (0..prefix_columns)
            .map(|column| {
                let cap = if column == 0 {
                    36
                } else if column == prefix_columns - 1 {
                    11
                } else {
                    20
                };

                cells
                    .iter()
                    .map(|row| measure_text_width(&row[column].0))
                    .chain([measure_text_width(&headings[column])])
                    .max()
                    .unwrap_or(1)
                    .min(cap)
                    .max(1)
            })
            .collect::<Vec<_>>();

        // Reserve space for the description, shrinking other columns on narrow terminals.
        while widths.iter().sum::<usize>()
            + prefix_columns
            + "Description".len().min(terminal_width)
            > terminal_width
        {
            let Some((column, _)) = widths
                .iter()
                .enumerate()
                .filter(|(_, width)| **width > 1)
                .max_by_key(|(_, width)| **width)
            else {
                break;
            };

            widths[column] -= 1;
        }

        let description_width = terminal_width
            .saturating_sub(widths.iter().sum::<usize>() + prefix_columns)
            .max(1);

        widths.push(description_width);

        let render = |cells: &[(String, bool)]| {
            let rendered = cells
                .iter()
                .enumerate()
                .map(|(column, (value, numeric))| {
                    let width = widths[column];
                    let tail = ".".repeat(width.min(3));
                    let value = truncate_str(value, width, &tail);
                    if column == widths.len() - 1 {
                        value.into_owned()
                    } else {
                        pad_str(
                            &value,
                            width,
                            if *numeric {
                                Alignment::Right
                            } else {
                                Alignment::Left
                            },
                            None,
                        )
                        .into_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            truncate_str(&rendered, terminal_width, "").into_owned()
        };

        let header = render(
            &headings
                .into_iter()
                .map(|heading| (heading, false))
                .collect::<Vec<_>>(),
        );

        Self {
            header,
            divider: "-".repeat(terminal_width),
            rows: cells.iter().map(|row| render(row)).collect(),
        }
    }
}

fn clean(value: &str) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() { "-".into() } else { value }
}

fn heading(key: &str) -> String {
    let key = key.replace('_', " ");
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn format_count(count: u64) -> String {
    if count < 1_000 {
        return count.to_string();
    }

    let (divisor, suffix) = if count < 1_000_000 {
        (1_000.0, "k")
    } else {
        (1_000_000.0, "m")
    };

    let scaled = count as f64 / divisor;
    if scaled >= 100.0 || scaled.fract() == 0.0 {
        format!("{scaled:.0}{suffix}")
    } else {
        format!("{scaled:.1}{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> SearchTableRow {
        SearchTableRow {
            source: "Krita (2259793)".into(),
            metadata: Vec::new(),
            updated: "today".into(),
            description: "x".repeat(400),
        }
    }

    #[test]
    fn description_uses_remaining_width_and_catalogs_have_no_forge_columns() {
        for width in [80, 140, 200] {
            let table = SearchTable::new("Name / ID", &[row()], width);
            assert!(!table.header.contains("Stars"));
            assert!(!table.header.contains("Lang"));
            assert!(table.header.contains("Updated"));
            assert_eq!(measure_text_width(&table.divider), width);
            assert_eq!(measure_text_width(&table.rows[0]), width);
            if width > 100 {
                assert!(table.rows[0].matches('x').count() > 72);
            }
        }
    }

    #[test]
    fn arbitrary_metadata_defines_ordered_columns_and_typed_values() {
        let mut first = row();
        first.metadata = vec![
            ("downloads".into(), SearchMetadataValue::Integer(1_250)),
            ("license".into(), SearchMetadataValue::Text("MIT".into())),
        ];
        let table = SearchTable::new("Name / ID", &[first, row()], 120);
        assert!(table.header.find("Downloads").unwrap() < table.header.find("License").unwrap());
        assert!(table.rows[0].contains("1.2k"));
        assert!(table.rows[0].contains("MIT"));
        assert!(table.rows[1].contains('-'));
    }

    #[test]
    fn unicode_and_narrow_terminal_rows_do_not_overflow() {
        let mut row = row();
        row.source = "漢字".repeat(30);
        row.description = "🌍 世界 ".repeat(100);
        row.metadata = vec![("lang".into(), SearchMetadataValue::Text("日本語".into()))];
        for width in [1, 10, 20, 40, 80] {
            let table = SearchTable::new("Name / ID", std::slice::from_ref(&row), width);
            assert!(measure_text_width(&table.header) <= width);
            assert!(measure_text_width(&table.rows[0]) <= width);
        }
    }

    #[test]
    fn format_count_uses_compact_suffixes() {
        assert_eq!(format_count(561), "561");
        assert_eq!(format_count(1_000), "1k");
        assert_eq!(format_count(9_645), "9.6k");
        assert_eq!(format_count(63_520), "63.5k");
        assert_eq!(format_count(1_250_000), "1.2m");
    }
}
