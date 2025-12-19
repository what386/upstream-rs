mod app;
mod models;
mod services;
mod utils;

use clap::Parser;
use anyhow::Result;

use {
    app::{
        cli::args::Cli,
    },
};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(err) = cli.run().await {

        #[cfg(debug_assertions)]
        {
            eprintln!("{:?}", err);
        }

        #[cfg(not(debug_assertions))]
        {
            eprintln!(
            "{}",
            err.chain()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
            );
        }

        std::process::exit(1);
    }
}
