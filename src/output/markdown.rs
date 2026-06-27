use std::{
    io::Write as _,
    process::{Command, Stdio},
};

use console::Term;

pub struct MarkdownRenderer {
    enabled: bool,
    width: usize,
    command: String,
    style: String,
}

impl MarkdownRenderer {
    pub fn for_terminal() -> Self {
        Self::new(terminal_width())
    }

    pub fn new(width: usize) -> Self {
        let command = std::env::var("UPSTREAM_GLOW_COMMAND").unwrap_or_else(|_| "glow".to_string());
        Self {
            enabled: glow_is_available(&command),
            width,
            command,
            style: std::env::var("UPSTREAM_GLOW_STYLE").unwrap_or_else(|_| "dark".to_string()),
        }
    }

    #[cfg(test)]
    pub fn plain() -> Self {
        Self {
            enabled: false,
            width: 80,
            command: "glow".to_string(),
            style: "dark".to_string(),
        }
    }

    pub fn render(&self, markdown: &str) -> String {
        if !self.enabled {
            return markdown.to_string();
        }

        self.render_with_glow(markdown)
            .filter(|output| !output.trim().is_empty())
            .unwrap_or_else(|| markdown.to_string())
    }

    fn render_with_glow(&self, markdown: &str) -> Option<String> {
        let mut child = Command::new(&self.command)
            .arg("-s")
            .arg(&self.style)
            .arg("-w")
            .arg(self.width.to_string())
            .arg("-n")
            .arg("-")
            .env_remove("NO_COLOR")
            .env("CLICOLOR_FORCE", "1")
            .env("FORCE_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        child.stdin.as_mut()?.write_all(markdown.as_bytes()).ok()?;
        drop(child.stdin.take());

        let output = child.wait_with_output().ok()?;
        if !output.status.success() {
            return None;
        }

        String::from_utf8(output.stdout)
            .ok()
            .map(normalize_glow_output)
    }
}

fn glow_is_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn normalize_glow_output(output: String) -> String {
    let mut lines = output.lines().collect::<Vec<_>>();

    while lines
        .first()
        .is_some_and(|line| console::strip_ansi_codes(line).trim().is_empty())
    {
        lines.remove(0);
    }
    while lines
        .last()
        .is_some_and(|line| console::strip_ansi_codes(line).trim().is_empty())
    {
        lines.pop();
    }

    lines
        .into_iter()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn terminal_width() -> usize {
    let (_, cols) = Term::stdout().size();
    (cols as usize).max(20)
}

#[cfg(test)]
mod tests {
    use super::{MarkdownRenderer, normalize_glow_output};

    #[test]
    fn markdown_renderer_falls_back_when_glow_is_missing() {
        let renderer = MarkdownRenderer {
            enabled: true,
            width: 80,
            command: "upstream-test-missing-glow-command".to_string(),
            style: "dark".to_string(),
        };

        assert_eq!(renderer.render("# Heading\n"), "# Heading\n");
    }

    #[test]
    fn normalize_glow_output_trims_outer_blank_padding() {
        let output = "\n\x1b[1mTitle\x1b[0m   \n\nbody   \n\n".to_string();

        assert_eq!(normalize_glow_output(output), "\x1b[1mTitle\x1b[0m\n\nbody");
    }
}
