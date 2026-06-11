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
