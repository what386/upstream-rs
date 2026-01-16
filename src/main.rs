mod application;
mod models;
mod services;
mod providers;
mod utils;

use console::style;

use clap::Parser;

use application::cli::arguments::Cli;

/*
#[cfg(target_os = "windows")]
compile_error!(
    "Upstream is planned to be *Nix-only. If you'd like something similar, try out Scoop! (https://scoop.sh/)"
);
*/

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(err) = cli.run().await {
        #[cfg(debug_assertions)]
        {
            eprintln!("{:?}", style(err).red());
        }

        #[cfg(not(debug_assertions))]
        {
            eprintln!(
                "{}",
                style(
                    err.chain()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join("\n")
                )
                .red()
            );
        }

        std::process::exit(1);
    }
}
