use anyhow::{Result, anyhow};

use crate::{
    application::operations::hooks_operation::{check, cleanup, initialize, purge_data},
    utils::static_paths::UpstreamPaths,
};

pub fn run(cleanup_option: bool, check_option: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;

    if check_option {
        let report = check(&paths)?;
        for line in &report.messages {
            println!("{}", line);
        }

        if report.ok {
            println!("Initialization check passed.");
            return Ok(());
        }

        return Err(anyhow!("Initialization check failed"));
    }

    if cleanup_option {
        cleanup(&paths)?;
        println!("Removed upstream shell integration hooks.")
    } else {
        initialize(&paths)?;
        println!("Initialized upstream.")
    }

    Ok(())
}

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

pub fn run_hooks_purge(yes: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    if !yes && !confirm_purge(&paths)? {
        println!("Purge cancelled.");
        return Ok(());
    }

    cleanup(&paths)?;
    purge_data(&paths)?;
    println!(
        "Removed upstream shell integration hooks and deleted '{}'.",
        paths.dirs.data_dir.display()
    );
    Ok(())
}

fn confirm_purge(paths: &UpstreamPaths) -> Result<bool> {
    print!(
        "Delete upstream data directory '{}' and remove shell hooks? Type 'yes' to continue: ",
        paths.dirs.data_dir.display()
    );

    use std::io::{self, Write};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("yes"))
}
