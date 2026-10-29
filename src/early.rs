//! Handle requests that bypass the runtime, configuration, migrations, locks, and logging.
use anyhow::Result;
use clap::Parser;

use upstream_rs::{
    application::{
        cli::arguments::{Cli, Commands},
        commands::updates,
    },
    completion,
    utils::static_paths::UpstreamPaths,
};

pub fn run() -> Result<Option<Cli>> {
    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(index) = raw_args.iter().position(|arg| arg == "__complete") {
        completion::run(&raw_args[index + 1..])?;
        return Ok(None);
    }

    let cli = Cli::parse();
    match cli.command {
        Commands::Updates { notify: true, .. } => {
            if let Ok(paths) = UpstreamPaths::new() {
                updates::run_notification(&paths);
            }
        }
        Commands::Updates { json, .. } => {
            updates::run(json, false, &UpstreamPaths::new()?)?;
        }
        _ => return Ok(Some(cli)),
    }

    Ok(None)
}
