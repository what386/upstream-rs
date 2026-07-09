use clap::Parser;
use console::style;

use upstream_rs::application::cli::arguments::Cli;
use upstream_rs::routines::migrate;
use upstream_rs::storage::system::lock::LockStorage;
use upstream_rs::utils::static_paths::UpstreamPaths;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        print_error(&err);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let paths = UpstreamPaths::new()?;

    run_startup_migrations(&paths)?;

    cli.run(paths).await?;

    Ok(())
}

fn run_startup_migrations(paths: &UpstreamPaths) -> anyhow::Result<()> {
    let _startup_lock = LockStorage::acquire(paths, "startup migrate")?;

    migrate::run(paths)?;

    Ok(())
}

fn print_error(err: &anyhow::Error) {
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
}
