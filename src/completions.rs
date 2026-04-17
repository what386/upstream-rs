use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::{Generator, Shell, generate};
use std::io;
use upstream_rs::application::cli::arguments::Cli;

#[derive(Parser, Debug)]
#[command(
    name = "upstream-completions",
    about = "Generate shell completions for upstream"
)]
struct CompletionArgs {
    #[arg(value_enum)]
    shell: CompletionShell,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    Powershell,
    Zsh,
}

fn main() {
    let args = CompletionArgs::parse();
    let mut cmd = Cli::command();
    let mut out = io::stdout();

    match args.shell {
        CompletionShell::Bash => generate_for_shell(Shell::Bash, &mut cmd, &mut out),
        CompletionShell::Elvish => generate_for_shell(Shell::Elvish, &mut cmd, &mut out),
        CompletionShell::Fish => generate_for_shell(Shell::Fish, &mut cmd, &mut out),
        CompletionShell::Powershell => generate_for_shell(Shell::PowerShell, &mut cmd, &mut out),
        CompletionShell::Zsh => generate_for_shell(Shell::Zsh, &mut cmd, &mut out),
    }
}

fn generate_for_shell<G: Generator>(
    generator: G,
    cmd: &mut clap::Command,
    out: &mut dyn io::Write,
) {
    generate(generator, cmd, "upstream", out);
}
