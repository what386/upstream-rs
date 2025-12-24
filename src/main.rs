mod application;
mod models;
mod services;
mod utils;

// TODO: Initialization (setting up PATH and things)

use clap::Parser;

use application::cli::arguments::Cli;

#[cfg(target_os = "windows")]
// INFO: will assess whether windows support would be useful. leaning towards no since better options exist
compile_error!(
    "Upstream is planned to be *Nix-only. If you'd like something similar, try out Scoop! (https://scoop.sh/)"
);

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
