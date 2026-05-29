use anyhow::{Result, anyhow};

use crate::{
    application::operations::hooks_operation::{check, cleanup, initialize, purge_data},
    application::output,
    utils::static_paths::UpstreamPaths,
};

pub fn run_hooks_init() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    initialize(&paths)?;
    println!(
        "{}",
        output::success("Hooks complete: shell integration initialized.")
    );
    Ok(())
}

pub fn run_hooks_check() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let report = check(&paths)?;
    println!("{}", output::title("Hooks check"));
    for line in &report.messages {
        output::action_note(line);
    }

    if report.ok {
        println!("{}", output::success("Hooks check passed."));
        return Ok(());
    }

    Err(anyhow!("Hook check failed"))
}

pub fn run_hooks_clean() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    cleanup(&paths)?;
    println!(
        "{}",
        output::success("Hooks complete: shell integration removed.")
    );
    Ok(())
}

pub fn run_hooks_purge() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    output::confirm_or_cancel(format!(
        "Delete upstream data directory '{}' and remove shell hooks?",
        paths.dirs.data_dir.display()
    ), false)?;

    cleanup(&paths)?;
    purge_data(&paths)?;
    println!("{}", output::success("Hooks purge complete."));
    output::action_note(format!("Deleted '{}'", paths.dirs.data_dir.display()));
    Ok(())
}
