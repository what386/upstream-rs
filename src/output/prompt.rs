use console::{Key, Term, style};

use super::style::truncate_visible;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

const SELECTION_SCROLL_MARGIN: usize = 1;
const MAX_PREVIEW_SELECTION_ROWS: usize = 8;
const PREVIEW_CHROME_ROWS: usize = 3;
const FOOTER_ROWS: usize = 1;

static ASSUME_YES: AtomicBool = AtomicBool::new(false);

pub fn set_assume_yes(value: bool) {
    ASSUME_YES.store(value, Ordering::Relaxed);
}

pub fn assume_yes() -> bool {
    ASSUME_YES.load(Ordering::Relaxed)
}

fn confirm_impl(prompt: impl fmt::Display, default_yes: bool) -> anyhow::Result<bool> {
    if assume_yes() {
        return Ok(true);
    }

    if !io::stdin().is_terminal() {
        anyhow::bail!(
            "Confirmation required for non-interactive input. Re-run with --yes to continue."
        );
    }

    let suffix = if default_yes { " [Y/n] " } else { " [y/N]: " };
    print!("{prompt}{suffix}");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_ascii_lowercase();
    Ok(match normalized.as_str() {
        "y" | "yes" => true,
        "" => default_yes,
        _ => false,
    })
}

pub fn confirm_or_cancel(prompt: impl fmt::Display, default_yes: bool) -> anyhow::Result<()> {
    if confirm_impl(prompt, default_yes)? {
        return Ok(());
    }
    anyhow::bail!("Cancelled")
}

pub fn prompt_text(prompt: impl fmt::Display, default: Option<&str>) -> anyhow::Result<String> {
    if !io::stdin().is_terminal() {
        anyhow::bail!("Text input requires a terminal.");
    }

    let suffix = default
        .map(|value| format!(" [{value}] "))
        .unwrap_or_else(|| ": ".to_string());
    print!("{prompt}{suffix}");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    resolve_text_prompt_value(input.trim(), default)
}

fn resolve_text_prompt_value(input: &str, default: Option<&str>) -> anyhow::Result<String> {
    if input.is_empty()
        && let Some(default) = default
    {
        return Ok(default.to_string());
    }

    let value = input.trim();
    if value.is_empty() {
        anyhow::bail!("Input cannot be empty");
    }

    Ok(value.to_string())
}

pub fn select_from_list(
    prompt: impl fmt::Display,
    items: &[String],
) -> anyhow::Result<Option<usize>> {
    select_from_table(prompt, &[], items)
}

pub fn select_from_table(
    prompt: impl fmt::Display,
    headers: &[String],
    items: &[String],
) -> anyhow::Result<Option<usize>> {
    select_from_table_impl(prompt, headers, items, None)
}

pub fn select_from_table_with_preview(
    prompt: impl fmt::Display,
    headers: &[String],
    items: &[String],
    previews: &[String],
) -> anyhow::Result<Option<usize>> {
    if items.len() != previews.len() {
        anyhow::bail!(
            "Interactive selection preview count ({}) does not match item count ({})",
            previews.len(),
            items.len()
        );
    }

    select_from_table_impl(prompt, headers, items, Some(previews))
}

fn select_from_table_impl(
    prompt: impl fmt::Display,
    headers: &[String],
    items: &[String],
    previews: Option<&[String]>,
) -> anyhow::Result<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }

    let term = Term::stdout();
    if !term.is_term() || !io::stdin().is_terminal() {
        anyhow::bail!("Interactive selection requires a terminal.");
    }

    select_from_list_with_term(&term, &prompt.to_string(), headers, items, previews)
}

fn select_from_list_with_term(
    term: &Term,
    prompt: &str,
    headers: &[String],
    items: &[String],
    previews: Option<&[String]>,
) -> anyhow::Result<Option<usize>> {
    let mut selected = 0;
    let mut top = 0;
    let mut rendered_lines = 0;

    loop {
        if rendered_lines > 0 {
            clear_rendered_selection(term, rendered_lines)?;
        }
        rendered_lines =
            render_selection(term, prompt, headers, items, selected, &mut top, previews)?;

        match selection_action_for_key(term.read_key()?) {
            SelectionAction::Accept => {
                term.clear_line()?;
                return Ok(Some(selected));
            }
            SelectionAction::Cancel => {
                term.clear_line()?;
                return Ok(None);
            }
            SelectionAction::Next => selected = (selected + 1) % items.len(),
            SelectionAction::Previous => {
                selected = if selected == 0 {
                    items.len() - 1
                } else {
                    selected - 1
                };
            }
            SelectionAction::Ignore => {}
        }
    }
}

fn render_selection(
    term: &Term,
    prompt: &str,
    headers: &[String],
    items: &[String],
    selected: usize,
    top: &mut usize,
    previews: Option<&[String]>,
) -> anyhow::Result<usize> {
    let (rows, cols) = term.size();
    let term_rows = rows as usize;
    let cols = cols as usize;
    let prompt_lines = prompt_lines(prompt);
    let preview_lines = selected_preview_lines(previews, selected);
    let preview_line_count = max_preview_line_count(previews);
    let layout = selection_preview_layout(
        term_rows,
        prompt_lines.len(),
        headers.len(),
        items.len(),
        preview_line_count,
    );
    let visible_rows = layout.selection_rows;
    *top = selection_top(selected, *top, visible_rows, items.len());
    let bottom = top.saturating_add(visible_rows).min(items.len());
    let mut rendered = 0;

    for line in prompt_lines {
        term.write_line(&style(truncate_width(&line, cols)).cyan().bold().to_string())?;
        rendered += 1;
    }

    for (index, header) in headers.iter().enumerate() {
        let line = truncate_width(header, cols);
        if index == 0 {
            term.write_line(&style(line).bold().to_string())?;
        } else {
            term.write_line(&line)?;
        }
        rendered += 1;
    }

    for (index, item) in items.iter().enumerate().take(bottom).skip(*top) {
        let marker = selection_marker(index, selected, *top, bottom, items.len());
        let line = truncate_width(&format!("{marker} {item}"), cols);
        if index == selected {
            term.write_line(&style(line).reverse().to_string())?;
        } else {
            term.write_line(&line)?;
        }
        rendered += 1;
    }

    if layout.preview_rows > 0 {
        term.write_line("")?;
        rendered += 1;
        term.write_line(&style("Preview").bold().to_string())?;
        rendered += 1;
        term.write_line(&truncate_width(&"-".repeat(cols.min(72)), cols))?;
        rendered += 1;
        for line in preview_lines
            .iter()
            .map(String::as_str)
            .chain(std::iter::repeat(""))
            .take(layout.preview_rows)
        {
            term.write_line(&truncate_width(line, cols))?;
            rendered += 1;
        }
    }

    let footer = truncate_width(
        &format!(
            "-- {}-{}/{} -- Enter:select  j/k or arrows:move  q/Esc:cancel",
            *top + 1,
            bottom,
            items.len()
        ),
        cols,
    );
    term.write_str(&style(footer).dim().to_string())?;
    rendered += 1;

    Ok(rendered)
}

fn selected_preview_lines(previews: Option<&[String]>, selected: usize) -> Vec<String> {
    previews
        .and_then(|values| values.get(selected))
        .map(|preview| preview.lines().map(ToString::to_string).collect())
        .unwrap_or_default()
}

fn max_preview_line_count(previews: Option<&[String]>) -> usize {
    previews
        .map(|values| {
            values
                .iter()
                .map(|preview| preview.lines().count())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn prompt_lines(prompt: &str) -> Vec<String> {
    let lines = prompt.lines().map(ToString::to_string).collect::<Vec<_>>();
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn preview_content_rows(
    term_rows: usize,
    prompt_rows: usize,
    header_rows: usize,
    selection_rows: usize,
    preview_line_count: usize,
) -> usize {
    if preview_line_count == 0 {
        return 0;
    }

    let used_rows = prompt_rows
        .saturating_add(header_rows)
        .saturating_add(selection_rows)
        .saturating_add(PREVIEW_CHROME_ROWS)
        .saturating_add(FOOTER_ROWS);
    term_rows.saturating_sub(used_rows).min(preview_line_count)
}

fn selection_visible_rows(term_rows: usize, fixed_rows: usize, item_count: usize) -> usize {
    term_rows.saturating_sub(fixed_rows).max(1).min(item_count)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectionPreviewLayout {
    selection_rows: usize,
    preview_rows: usize,
}

fn selection_preview_layout(
    term_rows: usize,
    prompt_rows: usize,
    header_rows: usize,
    item_count: usize,
    preview_line_count: usize,
) -> SelectionPreviewLayout {
    if item_count == 0 {
        return SelectionPreviewLayout {
            selection_rows: 0,
            preview_rows: 0,
        };
    }

    if preview_line_count == 0 {
        return SelectionPreviewLayout {
            selection_rows: selection_visible_rows(
                term_rows,
                prompt_rows + header_rows + FOOTER_ROWS,
                item_count,
            ),
            preview_rows: 0,
        };
    }

    let fixed_rows = prompt_rows + header_rows + PREVIEW_CHROME_ROWS + FOOTER_ROWS;
    let available_rows = term_rows.saturating_sub(fixed_rows);
    if available_rows < 2 {
        return SelectionPreviewLayout {
            selection_rows: selection_visible_rows(
                term_rows,
                prompt_rows + header_rows + FOOTER_ROWS,
                item_count,
            ),
            preview_rows: 0,
        };
    }

    let selection_rows = item_count
        .min(MAX_PREVIEW_SELECTION_ROWS)
        .min(available_rows - 1)
        .max(1);
    let preview_rows = preview_content_rows(
        term_rows,
        prompt_rows,
        header_rows,
        selection_rows,
        preview_line_count,
    );

    SelectionPreviewLayout {
        selection_rows,
        preview_rows,
    }
}

fn selection_top(
    selected: usize,
    current_top: usize,
    visible_rows: usize,
    item_count: usize,
) -> usize {
    if item_count <= visible_rows {
        return 0;
    }

    let last_top = item_count.saturating_sub(visible_rows);
    if visible_rows <= SELECTION_SCROLL_MARGIN * 2 {
        return selected.min(last_top);
    }

    let current_top = current_top.min(last_top);
    let top_margin_end = current_top + SELECTION_SCROLL_MARGIN;
    if selected < top_margin_end {
        return selected.saturating_sub(SELECTION_SCROLL_MARGIN);
    }

    let bottom_margin_start = current_top + visible_rows - SELECTION_SCROLL_MARGIN - 1;
    if selected > bottom_margin_start {
        return selected
            .saturating_sub(visible_rows - SELECTION_SCROLL_MARGIN - 1)
            .min(last_top);
    }

    current_top
}

fn selection_marker(
    index: usize,
    selected: usize,
    top: usize,
    bottom: usize,
    item_count: usize,
) -> &'static str {
    if index == selected {
        ">"
    } else if index == top && top > 0 {
        "^"
    } else if index + 1 == bottom && bottom < item_count {
        "v"
    } else {
        " "
    }
}

fn clear_rendered_selection(term: &Term, rendered_lines: usize) -> anyhow::Result<()> {
    term.clear_line()?;
    if rendered_lines > 1 {
        term.clear_last_lines(rendered_lines - 1)?;
    }
    Ok(())
}

fn truncate_width(value: &str, cols: usize) -> String {
    truncate_visible(value, cols)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionAction {
    Accept,
    Cancel,
    Next,
    Previous,
    Ignore,
}

fn selection_action_for_key(key: Key) -> SelectionAction {
    match key {
        Key::Enter => SelectionAction::Accept,
        Key::Char('q') | Key::Escape | Key::CtrlC => SelectionAction::Cancel,
        Key::Char('j') | Key::ArrowDown => SelectionAction::Next,
        Key::Char('k') | Key::ArrowUp => SelectionAction::Previous,
        _ => SelectionAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SelectionAction, SelectionPreviewLayout, max_preview_line_count, prompt_lines,
        resolve_text_prompt_value, selection_action_for_key, selection_marker,
        selection_preview_layout, selection_top, selection_visible_rows,
    };
    use console::Key;

    #[test]
    fn selection_keys_map_to_actions() {
        assert_eq!(
            selection_action_for_key(Key::Enter),
            SelectionAction::Accept
        );
        assert_eq!(
            selection_action_for_key(Key::Char('q')),
            SelectionAction::Cancel
        );
        assert_eq!(
            selection_action_for_key(Key::Escape),
            SelectionAction::Cancel
        );
        assert_eq!(
            selection_action_for_key(Key::ArrowDown),
            SelectionAction::Next
        );
        assert_eq!(
            selection_action_for_key(Key::Char('j')),
            SelectionAction::Next
        );
        assert_eq!(
            selection_action_for_key(Key::ArrowUp),
            SelectionAction::Previous
        );
        assert_eq!(
            selection_action_for_key(Key::Char('k')),
            SelectionAction::Previous
        );
        assert_eq!(
            selection_action_for_key(Key::Unknown),
            SelectionAction::Ignore
        );
    }

    #[test]
    fn selection_window_stays_inside_terminal_rows() {
        assert_eq!(selection_visible_rows(24, 2, 100), 22);
        assert_eq!(selection_visible_rows(24, 3, 100), 21);
        assert_eq!(selection_visible_rows(2, 2, 100), 1);
        assert_eq!(selection_visible_rows(24, 2, 3), 3);
    }

    #[test]
    fn preview_rows_leave_room_for_selection() {
        assert_eq!(
            selection_preview_layout(24, 1, 2, 100, 20),
            SelectionPreviewLayout {
                selection_rows: 8,
                preview_rows: 9,
            }
        );
        assert_eq!(
            selection_preview_layout(12, 1, 2, 100, 20),
            SelectionPreviewLayout {
                selection_rows: 4,
                preview_rows: 1,
            }
        );
        assert_eq!(
            selection_preview_layout(9, 1, 2, 100, 20),
            SelectionPreviewLayout {
                selection_rows: 1,
                preview_rows: 1,
            }
        );
        assert_eq!(
            selection_preview_layout(12, 2, 2, 100, 20),
            SelectionPreviewLayout {
                selection_rows: 3,
                preview_rows: 1,
            }
        );
    }

    #[test]
    fn preview_rows_grow_on_taller_terminals() {
        assert_eq!(
            selection_preview_layout(40, 2, 2, 100, 30),
            SelectionPreviewLayout {
                selection_rows: 8,
                preview_rows: 24,
            }
        );
        assert_eq!(
            selection_preview_layout(80, 2, 2, 100, 100),
            SelectionPreviewLayout {
                selection_rows: 8,
                preview_rows: 64,
            }
        );
        assert_eq!(
            selection_preview_layout(40, 2, 2, 100, 6),
            SelectionPreviewLayout {
                selection_rows: 8,
                preview_rows: 6,
            }
        );
    }

    #[test]
    fn preview_selection_list_is_capped_to_eight_rows() {
        assert_eq!(
            selection_preview_layout(80, 2, 2, 100, 100).selection_rows,
            8
        );
        assert_eq!(selection_preview_layout(80, 2, 2, 5, 100).selection_rows, 5);
    }

    #[test]
    fn preview_height_uses_largest_preview_for_stable_layout() {
        let previews = vec![
            "short".to_string(),
            "one\ntwo\nthree\nfour\nfive".to_string(),
            String::new(),
        ];

        assert_eq!(max_preview_line_count(Some(&previews)), 5);
        assert_eq!(max_preview_line_count(None), 0);
    }

    #[test]
    fn prompt_lines_preserve_multiline_headers() {
        assert_eq!(prompt_lines("package: rg\nqueries: usage").len(), 2);
        assert_eq!(prompt_lines("").len(), 1);
    }

    #[test]
    fn selection_top_sticks_to_bottom_margin_when_moving_down() {
        let mut top = 0;
        let visible_rows = 5;
        let item_count = 20;

        for selected in 0..=5 {
            top = selection_top(selected, top, visible_rows, item_count);
        }

        assert_eq!(top, 2);
        assert_eq!(5 - top, 3);
    }

    #[test]
    fn selection_top_sticks_to_top_margin_when_moving_up() {
        let visible_rows = 5;
        let item_count = 20;
        let mut top = item_count - visible_rows;

        for selected in (14..=19).rev() {
            top = selection_top(selected, top, visible_rows, item_count);
        }

        assert_eq!(top, 13);
        assert_eq!(14 - top, 1);
    }

    #[test]
    fn selection_top_reaches_scroll_boundaries() {
        assert_eq!(selection_top(0, 5, 5, 20), 0);
        assert_eq!(selection_top(19, 14, 5, 20), 15);
        assert_eq!(selection_top(3, 0, 2, 10), 3);
    }

    #[test]
    fn selection_marker_shows_scroll_indicators_at_visible_edges() {
        assert_eq!(selection_marker(3, 4, 3, 8, 10), "^");
        assert_eq!(selection_marker(7, 4, 3, 8, 10), "v");
        assert_eq!(selection_marker(4, 4, 3, 8, 10), ">");
        assert_eq!(selection_marker(0, 1, 0, 5, 10), " ");
        assert_eq!(selection_marker(9, 8, 5, 10, 10), " ");
    }

    #[test]
    fn text_prompt_uses_default_for_empty_input() {
        assert_eq!(
            resolve_text_prompt_value("", Some("ripgrep")).expect("resolve prompt"),
            "ripgrep"
        );
        assert_eq!(
            resolve_text_prompt_value("rg", Some("ripgrep")).expect("resolve prompt"),
            "rg"
        );
        assert!(resolve_text_prompt_value("", None).is_err());
    }
}
