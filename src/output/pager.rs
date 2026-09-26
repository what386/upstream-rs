use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use console::{Key, Term, style};

use super::style::truncate_visible;

const MIN_VISIBLE_ROWS: usize = 1;
const FOOTER_ROWS: usize = 1;

static NO_PAGER: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagerConfig {
    pub rows: usize,
    pub cols: usize,
}

impl PagerConfig {
    pub fn from_term(term: &Term) -> Self {
        let (rows, cols) = term.size();
        Self {
            rows: rows as usize,
            cols: cols as usize,
        }
    }

    fn content_rows(&self, header_rows: usize) -> usize {
        self.rows
            .saturating_sub(FOOTER_ROWS)
            .saturating_sub(header_rows)
            .max(MIN_VISIBLE_ROWS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagerAction {
    NextLine,
    PreviousLine,
    NextColumn,
    PreviousColumn,
    NextPage,
    PreviousPage,
    Top,
    Bottom,
    Quit,
    Interrupt,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagerState {
    top: usize,
    left: usize,
    total_lines: usize,
    visible_rows: usize,
    max_width: usize,
    visible_cols: usize,
}

impl PagerState {
    fn new(total_lines: usize, visible_rows: usize, max_width: usize, visible_cols: usize) -> Self {
        Self {
            top: 0,
            left: 0,
            total_lines,
            visible_rows: visible_rows.max(MIN_VISIBLE_ROWS),
            max_width,
            visible_cols,
        }
    }

    fn last_top(&self) -> usize {
        self.total_lines.saturating_sub(self.visible_rows)
    }

    fn last_left(&self) -> usize {
        self.max_width.saturating_sub(self.visible_cols)
    }

    fn apply(&mut self, action: PagerAction) {
        match action {
            PagerAction::NextLine => {
                self.top = (self.top + 1).min(self.last_top());
            }
            PagerAction::PreviousLine => {
                self.top = self.top.saturating_sub(1);
            }
            PagerAction::NextColumn => {
                self.left = (self.left + 1).min(self.last_left());
            }
            PagerAction::PreviousColumn => {
                self.left = self.left.saturating_sub(1);
            }
            PagerAction::NextPage => {
                self.top = self
                    .top
                    .saturating_add(self.visible_rows)
                    .min(self.last_top());
            }
            PagerAction::PreviousPage => {
                self.top = self.top.saturating_sub(self.visible_rows);
            }
            PagerAction::Top => {
                self.top = 0;
            }
            PagerAction::Bottom => {
                self.top = self.last_top();
            }
            PagerAction::Quit | PagerAction::Interrupt | PagerAction::Ignore => {}
        }
    }
}

pub fn set_no_pager(value: bool) {
    NO_PAGER.store(value, Ordering::Relaxed);
}

fn no_pager() -> bool {
    NO_PAGER.load(Ordering::Relaxed)
}

pub fn page_text(title: Option<&str>, text: &str) -> Result<()> {
    page_text_with_header(title, None, false, false, text)
}

pub fn page_table(header: &str, footer_right: &str, text: &str) -> Result<()> {
    page_text_with_header(Some(header), Some(footer_right), true, true, text)
}

fn page_text_with_header(
    header: Option<&str>,
    footer_right: Option<&str>,
    scroll_header_horizontally: bool,
    add_header_separator: bool,
    text: &str,
) -> Result<()> {
    if no_pager() {
        print_without_pager(header, text, add_header_separator)?;
        print_footer_metadata(footer_right);
        return Ok(());
    }

    let term = Term::stdout();
    if !term.is_term() {
        print_without_pager(header, text, add_header_separator)?;
        print_footer_metadata(footer_right);
        return Ok(());
    }

    let config = PagerConfig::from_term(&term);
    let lines = text.lines().map(ToString::to_string).collect::<Vec<_>>();
    let header_rows =
        header.map_or(0, |value| value.lines().count()) + usize::from(add_header_separator);
    let max_width = max_content_width(header, scroll_header_horizontally, &lines);

    if lines.len() <= config.content_rows(header_rows) && max_width <= config.cols {
        print_without_pager(header, text, add_header_separator)?;
        print_footer_metadata(footer_right);
        return Ok(());
    }

    page_lines(
        &term,
        header,
        footer_right,
        scroll_header_horizontally,
        add_header_separator,
        &lines,
        config,
    )
}

fn print_without_pager(header: Option<&str>, text: &str, add_header_separator: bool) -> Result<()> {
    if let Some(header) = header {
        for line in header.lines() {
            println!("{}", style(line).cyan().bold());
        }

        if add_header_separator {
            let cols = Term::stdout().size().1 as usize;
            println!("{}", "-".repeat(cols));
        }
    }

    print!("{text}");
    io::stdout().flush()?;
    Ok(())
}

fn print_footer_metadata(value: Option<&str>) {
    let Some(value) = value else {
        return;
    };

    let cols = Term::stdout().size().1 as usize;
    let value = truncate_width(value, cols);
    println!("{:>width$}", value, width = cols);
}

fn page_lines(
    term: &Term,
    header: Option<&str>,
    footer_right: Option<&str>,
    scroll_header_horizontally: bool,
    add_header_separator: bool,
    lines: &[String],
    config: PagerConfig,
) -> Result<()> {
    let max_width = max_content_width(header, scroll_header_horizontally, lines);

    let mut state = PagerState::new(
        lines.len(),
        config.content_rows(
            header.map_or(0, |value| value.lines().count()) + usize::from(add_header_separator),
        ),
        max_width,
        config.cols,
    );

    let mut rendered_lines = 0;

    loop {
        if rendered_lines > 0 {
            clear_rendered_view(term, rendered_lines)?;
        }

        rendered_lines = render_view(
            term,
            header,
            footer_right,
            scroll_header_horizontally,
            add_header_separator,
            lines,
            &state,
            config.cols,
        )?;

        let action = action_for_key(term.read_key_raw()?);
        if action == PagerAction::Quit {
            break;
        }

        if action == PagerAction::Interrupt {
            clear_rendered_view(term, rendered_lines)?;
            crate::application::cancellation::request();
            return Err(crate::application::cancellation::Cancelled.into());
        }

        state.apply(action);
    }

    if rendered_lines > 0 {
        clear_rendered_view(term, rendered_lines)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_view(
    term: &Term,
    header: Option<&str>,
    footer_right: Option<&str>,
    scroll_header_horizontally: bool,
    add_header_separator: bool,
    lines: &[String],
    state: &PagerState,
    cols: usize,
) -> Result<usize> {
    let mut rendered = 0;

    if let Some(header) = header {
        for line in header.lines() {
            let line = if scroll_header_horizontally {
                horizontal_window(line, state.left, cols)
            } else {
                truncate_width(line, cols)
            };

            term.write_line(&style(line).cyan().bold().to_string())?;
            rendered += 1;
        }

        if add_header_separator {
            term.write_line(&"-".repeat(cols))?;
            rendered += 1;
        }
    }

    for line in visible_lines(lines, state) {
        term.write_line(&horizontal_window(line, state.left, cols))?;
        rendered += 1;
    }

    let footer = footer_line(&footer_text(state), footer_right, cols);
    term.write_str(&style(footer).dim().to_string())?;
    rendered += 1;

    Ok(rendered)
}

fn clear_rendered_view(term: &Term, rendered_lines: usize) -> Result<()> {
    term.clear_line()?;
    if rendered_lines > 1 {
        term.clear_last_lines(rendered_lines - 1)?;
    }

    Ok(())
}

fn visible_lines<'a>(lines: &'a [String], state: &PagerState) -> &'a [String] {
    let end = state
        .top
        .saturating_add(state.visible_rows)
        .min(lines.len());

    &lines[state.top..end]
}

fn footer_text(state: &PagerState) -> String {
    let start = if state.total_lines == 0 {
        0
    } else {
        state.top + 1
    };

    let end = state
        .top
        .saturating_add(state.visible_rows)
        .min(state.total_lines);

    format!(
        "-- {start}-{end}/{} -- Space/PgDn:next b/PgUp:prev hjkl/arrows:scroll g:top G:bottom q:quit",
        state.total_lines
    )
}

fn footer_line(left: &str, right: Option<&str>, cols: usize) -> String {
    let Some(right) = right else {
        return truncate_width(left, cols);
    };

    let right = truncate_width(right, cols);
    let left_width = cols.saturating_sub(right.chars().count() + 1);
    let left = truncate_width(left, left_width);
    format!("{left:<left_width$} {right}")
}

fn truncate_width(value: &str, cols: usize) -> String {
    truncate_visible(value, cols)
}

fn max_content_width(
    header: Option<&str>,
    scroll_header_horizontally: bool,
    lines: &[String],
) -> usize {
    lines
        .iter()
        .map(|line| line.chars().count())
        .chain(if scroll_header_horizontally {
            header
                .into_iter()
                .flat_map(str::lines)
                .map(|line| line.chars().count())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        })
        .max()
        .unwrap_or_default()
}

fn horizontal_window(value: &str, left: usize, cols: usize) -> String {
    let window = value.chars().skip(left).take(cols).collect::<String>();
    truncate_width(&window, cols)
}

fn action_for_key(key: Key) -> PagerAction {
    match key {
        Key::Char('q') | Key::Escape => PagerAction::Quit,
        Key::CtrlC => PagerAction::Interrupt,
        Key::Char(' ') | Key::PageDown => PagerAction::NextPage,
        Key::Char('b') | Key::PageUp => PagerAction::PreviousPage,
        Key::Char('j') | Key::ArrowDown | Key::Enter => PagerAction::NextLine,
        Key::Char('k') | Key::ArrowUp => PagerAction::PreviousLine,
        Key::Char('h') | Key::ArrowLeft => PagerAction::PreviousColumn,
        Key::Char('l') | Key::ArrowRight => PagerAction::NextColumn,
        Key::Char('g') | Key::Home => PagerAction::Top,
        Key::Char('G') | Key::End => PagerAction::Bottom,
        _ => PagerAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PagerAction, PagerState, action_for_key, footer_line, footer_text, max_content_width,
        page_text, visible_lines,
    };
    use console::Key;

    fn lines(count: usize) -> Vec<String> {
        (1..=count).map(|line| format!("line {line}")).collect()
    }

    #[test]
    fn next_and_previous_page_clamp_to_bounds() {
        let mut state = PagerState::new(10, 3, 10, 10);
        state.apply(PagerAction::NextPage);
        assert_eq!(state.top, 3);
        state.apply(PagerAction::NextPage);
        assert_eq!(state.top, 6);
        state.apply(PagerAction::NextPage);
        assert_eq!(state.top, 7);
        state.apply(PagerAction::PreviousPage);
        assert_eq!(state.top, 4);
        state.apply(PagerAction::PreviousPage);
        assert_eq!(state.top, 1);
        state.apply(PagerAction::PreviousPage);
        assert_eq!(state.top, 0);
    }

    #[test]
    fn line_navigation_clamps_to_bounds() {
        let mut state = PagerState::new(4, 2, 10, 10);
        state.apply(PagerAction::PreviousLine);
        assert_eq!(state.top, 0);
        state.apply(PagerAction::NextLine);
        state.apply(PagerAction::NextLine);
        state.apply(PagerAction::NextLine);
        assert_eq!(state.top, 2);
    }

    #[test]
    fn horizontal_navigation_clamps_to_bounds() {
        let mut state = PagerState::new(1, 2, 12, 5);
        state.apply(PagerAction::PreviousColumn);
        assert_eq!(state.left, 0);
        for _ in 0..20 {
            state.apply(PagerAction::NextColumn);
        }

        assert_eq!(state.left, 7);
        state.apply(PagerAction::PreviousColumn);
        assert_eq!(state.left, 6);
    }

    #[test]
    fn top_and_bottom_jump_to_expected_offsets() {
        let mut state = PagerState::new(10, 4, 10, 10);
        state.apply(PagerAction::Bottom);
        assert_eq!(state.top, 6);
        state.apply(PagerAction::Top);
        assert_eq!(state.top, 0);
    }

    #[test]
    fn visible_lines_returns_current_window() {
        let lines = lines(5);
        let mut state = PagerState::new(lines.len(), 2, 10, 10);
        state.apply(PagerAction::NextPage);
        assert_eq!(visible_lines(&lines, &state), &lines[2..4]);
    }

    #[test]
    fn footer_describes_visible_range() {
        let mut state = PagerState::new(12, 5, 10, 10);
        state.apply(PagerAction::NextPage);
        assert!(footer_text(&state).starts_with("-- 6-10/12 --"));
    }

    #[test]
    fn footer_metadata_is_right_aligned() {
        let footer = footer_line("-- 1-5/12 --", Some("Packages (12)"), 40);
        assert_eq!(footer.chars().count(), 40);
        assert!(footer.ends_with("Packages (12)"));
    }

    #[test]
    fn max_content_width_includes_scrollable_header() {
        let lines = vec!["short".to_string()];

        assert_eq!(
            max_content_width(Some("a much longer header"), true, &lines),
            20
        );
        assert_eq!(
            max_content_width(Some("a much longer header"), false, &lines),
            5
        );
    }

    #[test]
    fn maps_less_like_keys_to_actions() {
        assert_eq!(action_for_key(Key::Char('q')), PagerAction::Quit);
        assert_eq!(action_for_key(Key::Char(' ')), PagerAction::NextPage);
        assert_eq!(action_for_key(Key::Char('b')), PagerAction::PreviousPage);
        assert_eq!(action_for_key(Key::Char('j')), PagerAction::NextLine);
        assert_eq!(action_for_key(Key::Char('k')), PagerAction::PreviousLine);
        assert_eq!(action_for_key(Key::Char('h')), PagerAction::PreviousColumn);
        assert_eq!(action_for_key(Key::Char('l')), PagerAction::NextColumn);
        assert_eq!(action_for_key(Key::ArrowLeft), PagerAction::PreviousColumn);
        assert_eq!(action_for_key(Key::ArrowRight), PagerAction::NextColumn);
        assert_eq!(action_for_key(Key::Char('g')), PagerAction::Top);
        assert_eq!(action_for_key(Key::Char('G')), PagerAction::Bottom);
        assert_eq!(action_for_key(Key::Unknown), PagerAction::Ignore);
    }

    #[test]
    #[ignore = "manual pager smoke test; run with --ignored --nocapture in a terminal"]
    fn manual_force_pager() {
        let mut text = String::new();
        for index in 1..=160 {
            text.push_str(&format!(
                "{index:03}  This is a manually generated pager test line with enough content to exercise truncation and navigation.\n"
            ));
        }

        page_text(Some("Manual pager smoke test"), &text).expect("pager should run");
    }
}
