use clap::Parser;
use console::style;

use upstream_rs::application::cancellation;
use upstream_rs::application::cli::arguments::Cli;
use upstream_rs::output;
use upstream_rs::routines::migrate;
use upstream_rs::storage::system::config::ConfigStorage;
use upstream_rs::storage::system::lock::LockStorage;
use upstream_rs::utils::static_paths::UpstreamPaths;

#[tokio::main]
async fn main() {
    let mut command = Box::pin(run());
    let result = tokio::select! {
        result = &mut command => result.map(|_| 0),
        signal = tokio::signal::ctrl_c() => {
            match signal {
                Ok(()) => {
                    cancellation::request();
                    eprintln!("CTRL-C received; cleaning up...");

                    tokio::select! {
                        result = &mut command => result.map(|_| 0),
                        second_signal = tokio::signal::ctrl_c() => {
                            if second_signal.is_ok() {
                                eprintln!("Second CTRL-C received; exiting immediately.");
                            }
                            std::process::exit(130);
                        }
                    }
                }
                Err(err) => Err(anyhow::anyhow!("Failed to install CTRL-C handler: {err}")),
            }
        }
    };

    match result {
        Ok(code) => {
            if cancellation::is_requested() {
                std::process::exit(130);
            }
            std::process::exit(code);
        }
        Err(err) if cancellation::is_requested() => {
            eprintln!("Interrupted: {}", output::error_summary(&err));
            std::process::exit(130);
        }
        Err(err) => {
            output::log_error(output::error_summary(&err));
            print_error(&err);
            std::process::exit(1);
        }
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
