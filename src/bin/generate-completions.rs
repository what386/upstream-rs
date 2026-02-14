use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::CommandFactory;
use clap_complete::{Generator, Shell, generate_to};
use upstream_rs::application::cli::arguments::Cli;

fn write_for_shell<G: Generator>(generator: G, out_dir: &PathBuf, bin_name: &str) -> Result<()> {
    let mut cmd = Cli::command();
    generate_to(generator, &mut cmd, bin_name, out_dir)
        .context("Failed to generate completion script")?;
    Ok(())
}

fn main() -> Result<()> {
    let out_dir = PathBuf::from("completions");
    fs::create_dir_all(&out_dir).context("Failed to create completions output directory")?;

    write_for_shell(Shell::Bash, &out_dir, "upstream")?;
    write_for_shell(Shell::Zsh, &out_dir, "upstream")?;
    write_for_shell(Shell::Fish, &out_dir, "upstream")?;
    write_for_shell(Shell::PowerShell, &out_dir, "upstream")?;

    println!("Generated shell completions in {}", out_dir.display());
    Ok(())
}
