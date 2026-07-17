use clap::Parser;
use console::style;

use upstream_rs::application::cli::arguments::Cli;
use upstream_rs::output;
use upstream_rs::routines::migrate;
use upstream_rs::storage::system::config::ConfigStorage;
use upstream_rs::storage::system::lock::LockStorage;
use upstream_rs::utils::static_paths::UpstreamPaths;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        output::log_error(output::error_summary(&err));
        print_error(&err);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let paths = UpstreamPaths::new()?;
    output::init_logger(paths.dirs.data_dir.join("log.jsonl"));
    output::set_log_command(cli.command.to_string());

    if let Err(err) = run_startup_migrations(&paths) {
        output::log_command_result(false, Some(output::error_summary(&err)));
        return Err(err);
    }

    let config = ConfigStorage::new(&paths.config.config_file)?;
    output::configure_logger(config.get_config().logging);

    match cli.run(paths).await {
        Ok(()) => output::log_command_result(true, None),
        Err(err) => {
            output::log_command_result(false, Some(output::error_summary(&err)));
            return Err(err);
        }
    }

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
