use std::io::{self, Write};

use anyhow::Result;
use console::{Key, Term};

const MIN_VISIBLE_ROWS: usize = 1;
const FOOTER_ROWS: usize = 1;

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

    fn visible_rows(&self) -> usize {
        self.rows.saturating_sub(FOOTER_ROWS).max(MIN_VISIBLE_ROWS)
    }

    fn content_rows(&self, has_title: bool) -> usize {
        let title_rows = usize::from(has_title);
        self.rows
            .saturating_sub(FOOTER_ROWS)
            .saturating_sub(title_rows)
            .max(MIN_VISIBLE_ROWS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagerAction {
    NextLine,
    PreviousLine,
    NextPage,
    PreviousPage,
    Top,
    Bottom,
    Quit,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagerState {
    top: usize,
    total_lines: usize,
    visible_rows: usize,
}

impl PagerState {
    fn new(total_lines: usize, visible_rows: usize) -> Self {
        Self {
            top: 0,
            total_lines,
            visible_rows: visible_rows.max(MIN_VISIBLE_ROWS),
        }
    }

    fn last_top(&self) -> usize {
        self.total_lines.saturating_sub(self.visible_rows)
    }

    fn apply(&mut self, action: PagerAction) {
        match action {
            PagerAction::NextLine => {
                self.top = (self.top + 1).min(self.last_top());
            }
            PagerAction::PreviousLine => {
                self.top = self.top.saturating_sub(1);
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
            PagerAction::Quit | PagerAction::Ignore => {}
        }
    }
}

pub fn should_page(line_count: usize) -> bool {
    let term = Term::stdout();
    term.is_term() && line_count > PagerConfig::from_term(&term).visible_rows()
}

pub fn page_text(title: Option<&str>, text: &str) -> Result<()> {
    let term = Term::stdout();
    if !term.is_term() {
        print!("{text}");
        io::stdout().flush()?;
        return Ok(());
    }

    let config = PagerConfig::from_term(&term);
    let lines = text.lines().map(ToString::to_string).collect::<Vec<_>>();
    if lines.len() <= config.visible_rows() {
        print!("{text}");
        io::stdout().flush()?;
        return Ok(());
    }

    page_lines(&term, title, &lines, config)
}

fn page_lines(
    term: &Term,
    title: Option<&str>,
    lines: &[String],
    config: PagerConfig,
) -> Result<()> {
    let mut state = PagerState::new(lines.len(), config.content_rows(title.is_some()));
    let mut rendered_lines = 0;

    loop {
        if rendered_lines > 0 {
            clear_rendered_view(term, rendered_lines)?;
        }
        rendered_lines = render_view(term, title, lines, &state, config.cols)?;

        let action = action_for_key(term.read_key()?);
        if action == PagerAction::Quit {
            break;
        }
        state.apply(action);
    }

    if rendered_lines > 0 {
        clear_rendered_view(term, rendered_lines)?;
    }
    Ok(())
}

fn render_view(
    term: &Term,
    title: Option<&str>,
    lines: &[String],
    state: &PagerState,
    cols: usize,
) -> Result<usize> {
    let mut rendered = 0;

    if let Some(title) = title {
        term.write_line(&truncate_width(title, cols))?;
        rendered += 1;
    }

    for line in visible_lines(lines, state) {
        term.write_line(&truncate_width(line, cols))?;
        rendered += 1;
    }

    term.write_str(&truncate_width(&footer_text(state), cols))?;
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
        "-- {start}-{end}/{} -- Space/PgDn:next b/PgUp:prev j/k:line g/G:top/bottom q:quit",
        state.total_lines
    )
}

fn truncate_width(value: &str, cols: usize) -> String {
    if cols == 0 {
        return String::new();
    }

    value.chars().take(cols).collect()
}

fn action_for_key(key: Key) -> PagerAction {
    match key {
        Key::Char('q') | Key::Escape | Key::CtrlC => PagerAction::Quit,
        Key::Char(' ') | Key::PageDown => PagerAction::NextPage,
        Key::Char('b') | Key::PageUp => PagerAction::PreviousPage,
        Key::Char('j') | Key::ArrowDown | Key::Enter => PagerAction::NextLine,
        Key::Char('k') | Key::ArrowUp => PagerAction::PreviousLine,
        Key::Char('g') | Key::Home => PagerAction::Top,
        Key::Char('G') | Key::End => PagerAction::Bottom,
        _ => PagerAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PagerAction, PagerState, action_for_key, footer_text, page_text, truncate_width,
        visible_lines,
    };
    use console::Key;

    fn lines(count: usize) -> Vec<String> {
        (1..=count).map(|line| format!("line {line}")).collect()
    }

    #[test]
    fn next_and_previous_page_clamp_to_bounds() {
        let mut state = PagerState::new(10, 3);
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
        let mut state = PagerState::new(4, 2);
        state.apply(PagerAction::PreviousLine);
        assert_eq!(state.top, 0);
        state.apply(PagerAction::NextLine);
        state.apply(PagerAction::NextLine);
        state.apply(PagerAction::NextLine);
        assert_eq!(state.top, 2);
    }

    #[test]
    fn top_and_bottom_jump_to_expected_offsets() {
        let mut state = PagerState::new(10, 4);
        state.apply(PagerAction::Bottom);
        assert_eq!(state.top, 6);
        state.apply(PagerAction::Top);
        assert_eq!(state.top, 0);
    }

    #[test]
    fn visible_lines_returns_current_window() {
        let lines = lines(5);
        let mut state = PagerState::new(lines.len(), 2);
        state.apply(PagerAction::NextPage);
        assert_eq!(visible_lines(&lines, &state), &lines[2..4]);
    }

    #[test]
    fn footer_describes_visible_range() {
        let mut state = PagerState::new(12, 5);
        state.apply(PagerAction::NextPage);
        assert!(footer_text(&state).starts_with("-- 6-10/12 --"));
    }

    #[test]
    fn truncates_to_terminal_width() {
        assert_eq!(truncate_width("abcdef", 3), "abc");
        assert_eq!(truncate_width("abcdef", 0), "");
    }

    #[test]
    fn maps_less_like_keys_to_actions() {
        assert_eq!(action_for_key(Key::Char('q')), PagerAction::Quit);
        assert_eq!(action_for_key(Key::Char(' ')), PagerAction::NextPage);
        assert_eq!(action_for_key(Key::Char('b')), PagerAction::PreviousPage);
        assert_eq!(action_for_key(Key::Char('j')), PagerAction::NextLine);
        assert_eq!(action_for_key(Key::Char('k')), PagerAction::PreviousLine);
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
