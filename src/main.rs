pub mod application;
pub mod models;
pub mod providers;
pub mod services;
pub mod utils;

use clap::Parser;
use console::style;

use crate::application::cli::arguments::Cli;

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
