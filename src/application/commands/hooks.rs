use anyhow::{Result, anyhow};

use crate::{
    application::operations::hooks_operation::{check, cleanup, initialize, purge_data},
    application::output,
    utils::static_paths::UpstreamPaths,
};

pub fn run_hooks_init() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    initialize(&paths)?;
    println!("Initialized upstream shell integration hooks.");
    Ok(())
}

pub fn run_hooks_check() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let report = check(&paths)?;
    for line in &report.messages {
        println!("{}", line);
    }

    if report.ok {
        println!("Hook check passed.");
        return Ok(());
    }

    Err(anyhow!("Hook check failed"))
}

pub fn run_hooks_clean() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    cleanup(&paths)?;
    println!("Removed upstream shell integration hooks.");
    Ok(())
}

pub fn run_hooks_purge() -> Result<()> {
    let paths = UpstreamPaths::new()?;
    output::confirm_or_cancel(format!(
        "Delete upstream data directory '{}' and remove shell hooks?",
        paths.dirs.data_dir.display()
    ))?;

    cleanup(&paths)?;
    purge_data(&paths)?;
    println!(
        "Removed upstream shell integration hooks and deleted '{}'.",
        paths.dirs.data_dir.display()
    );
    Ok(())
}
