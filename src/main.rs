use console::style;

use tracing::Instrument;
use upstream_rs::application::cancellation::{self, Cancellation};
use upstream_rs::application::cli::arguments::Cli;
use upstream_rs::logging;
use upstream_rs::output;
use upstream_rs::routines::migrate;
use upstream_rs::storage::system::config::ConfigStorage;
use upstream_rs::storage::system::lock::LockStorage;
use upstream_rs::utils::static_paths::UpstreamPaths;

mod early;

#[cfg(windows)]
use upstream_rs::services::packaging::PackageActivator;

fn main() {
    match early::run() {
        Ok(Some(cli)) => run_main(cli),
        Ok(None) => {}
        Err(error) => {
            print_error(&error);
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn run_main(cli: Cli) {
    let cancellation = cancellation::current();
    let signal_task = tokio::spawn(signal_supervisor(cancellation));
    let result = run(cli).await.map(|_| 0);
    signal_task.abort();

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
            print_error(&err);
            std::process::exit(1);
        }
    }
}

async fn signal_supervisor(cancellation: Cancellation) {
    if !wait_for_interrupt().await {
        return;
    }

    tracing::warn!("Interrupt received; cleaning up");
    cancellation.request();
    eprintln!("CTRL-C received; cleaning up...");

    if wait_for_interrupt().await {
        tracing::warn!("Second interrupt received; exiting immediately");
        eprintln!("Second CTRL-C received; exiting immediately.");
        std::process::exit(130);
    }
}

#[cfg(not(windows))]
async fn wait_for_interrupt() -> bool {
    tokio::signal::ctrl_c().await.is_ok()
}

#[cfg(windows)]
async fn wait_for_interrupt() -> bool {
    let Ok(mut ctrl_c) = tokio::signal::windows::ctrl_c() else {
        return false;
    };

    let Ok(mut ctrl_break) = tokio::signal::windows::ctrl_break() else {
        return false;
    };

    tokio::select! {
        result = ctrl_c.recv() => result.is_some(),
        result = ctrl_break.recv() => result.is_some(),
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let paths = UpstreamPaths::new()?;
    // Invalid configuration is reported below, after logging is available.
    let logging_config = logging::read_config(&paths.config.config_file).unwrap_or_default();
    if let Err(error) = logging::init(&paths.dirs.data_dir.join(logging::LOG_FILE), logging_config)
    {
        eprintln!("Warning: {error:#}");
    }

    // Keep command context available even when only errors are enabled.
    let span = tracing::error_span!("command", command = %cli.command, pid = std::process::id());
    async {
        let started = std::time::Instant::now();
        tracing::info!("Command started");
        let result = run_command(cli, &paths).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match &result {
            _ if cancellation::is_requested() => tracing::warn!(elapsed_ms, outcome = "cancelled", "Command cancelled"),
            Ok(()) => tracing::info!(elapsed_ms, outcome = "success", "Command completed"),
            Err(error) => tracing::error!(elapsed_ms, outcome = "failure", error = %format!("{error:#}"), "Command failed"),
        }

        result
    }
    .instrument(span)
    .await
}

async fn run_command(cli: Cli, paths: &UpstreamPaths) -> anyhow::Result<()> {
    #[cfg(windows)]
    if let Err(error) = PackageActivator::cleanup_transient_snapshots(paths) {
        eprintln!(
            "{}",
            output::warning(format!(
                "Could not remove a previous replacement snapshot: {error:#}.\n
                Run `upstream doctor --fix` after the conflicting process exits."
            ))
        );
    }

    run_startup_migrations(paths)?;
    tracing::debug!("Startup migrations complete");
    let config = ConfigStorage::new(&paths.config.config_file)?;
    cli.run(paths, config.get_config()).await
}

#[tracing::instrument(level = "debug", skip_all)]
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
