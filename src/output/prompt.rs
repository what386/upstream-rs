use console::{Key, Term, style};
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

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
    if input.is_empty() {
        if let Some(default) = default {
            return Ok(default.to_string());
        }
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
    if items.is_empty() {
        return Ok(None);
    }

    let term = Term::stdout();
    if !term.is_term() || !io::stdin().is_terminal() {
        anyhow::bail!("Interactive selection requires a terminal.");
    }

    select_from_list_with_term(&term, &prompt.to_string(), headers, items)
}

fn select_from_list_with_term(
    term: &Term,
    prompt: &str,
    headers: &[String],
    items: &[String],
) -> anyhow::Result<Option<usize>> {
    let mut selected = 0;
    let mut rendered_lines = 0;

    loop {
        if rendered_lines > 0 {
            clear_rendered_selection(term, rendered_lines)?;
        }
        rendered_lines = render_selection(term, prompt, headers, items, selected)?;

        match selection_action_for_key(term.read_key()?) {
            SelectionAction::Accept => {
                clear_rendered_selection(term, rendered_lines)?;
                return Ok(Some(selected));
            }
            SelectionAction::Cancel => {
                clear_rendered_selection(term, rendered_lines)?;
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
) -> anyhow::Result<usize> {
    let (rows, cols) = term.size();
    let cols = cols as usize;
    let fixed_rows = 2 + headers.len();
    let visible_rows = selection_visible_rows(rows as usize, fixed_rows, items.len());
    let top = selection_top(selected, visible_rows, items.len());
    let bottom = top.saturating_add(visible_rows).min(items.len());
    let mut rendered = 0;

    term.write_line(
        &style(truncate_width(prompt, cols))
            .cyan()
            .bold()
            .to_string(),
    )?;
    rendered += 1;

    for (index, header) in headers.iter().enumerate() {
        let line = truncate_width(header, cols);
        if index == 0 {
            term.write_line(&style(line).bold().to_string())?;
        } else {
            term.write_line(&line)?;
        }
        rendered += 1;
    }

    for (index, item) in items.iter().enumerate().take(bottom).skip(top) {
        let marker = if index == selected { ">" } else { " " };
        let line = truncate_width(&format!("{marker} {item}"), cols);
        if index == selected {
            term.write_line(&style(line).reverse().to_string())?;
        } else {
            term.write_line(&line)?;
        }
        rendered += 1;
    }

    let footer = truncate_width(
        &format!(
            "-- {}-{}/{} -- Enter:select  j/k or arrows:move  q/Esc:cancel",
            top + 1,
            bottom,
            items.len()
        ),
        cols,
    );
    term.write_str(&style(footer).dim().to_string())?;
    rendered += 1;

    Ok(rendered)
}

fn selection_visible_rows(term_rows: usize, fixed_rows: usize, item_count: usize) -> usize {
    term_rows.saturating_sub(fixed_rows).max(1).min(item_count)
}

fn selection_top(selected: usize, visible_rows: usize, item_count: usize) -> usize {
    if item_count <= visible_rows || selected < visible_rows {
        return 0;
    }

    selected
        .saturating_add(1)
        .saturating_sub(visible_rows)
        .min(item_count.saturating_sub(visible_rows))
}

fn clear_rendered_selection(term: &Term, rendered_lines: usize) -> anyhow::Result<()> {
    term.clear_line()?;
    if rendered_lines > 1 {
        term.clear_last_lines(rendered_lines - 1)?;
    }
    Ok(())
}

fn truncate_width(value: &str, cols: usize) -> String {
    if cols == 0 {
        return String::new();
    }

    value.chars().take(cols).collect()
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
        SelectionAction, resolve_text_prompt_value, selection_action_for_key, selection_top,
        selection_visible_rows,
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
    fn selection_top_tracks_selected_item() {
        assert_eq!(selection_top(0, 5, 20), 0);
        assert_eq!(selection_top(4, 5, 20), 0);
        assert_eq!(selection_top(5, 5, 20), 1);
        assert_eq!(selection_top(19, 5, 20), 15);
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
