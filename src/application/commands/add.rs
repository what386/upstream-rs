use anyhow::{Result, bail};

use crate::{
    application::commands::install,
    models::upstream::config::AppConfig,
    output::{self, Status},
    routines::registry::{self, FetchOutcome},
    utils::static_paths::UpstreamPaths,
};

pub async fn run(
    name: Option<String>,
    fetch: bool,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    if name.is_none() && !fetch {
        bail!("Package name is required unless --fetch is used");
    }

    if name.is_none() {
        let outcome = registry::fetch(paths, &app_config.registry.index_url).await?;
        output::status_line(
            Status::Ok,
            "registry",
            match outcome {
                FetchOutcome::Updated => "index refreshed",
                FetchOutcome::NotModified => "index already current",
            },
        );
        return Ok(());
    }

    let (plan, outcome) = registry::resolve(
        name.expect("name checked above"),
        fetch,
        paths,
        &app_config.registry.index_url,
    )
    .await?;
    if let Some(outcome) = outcome {
        output::status_line(
            Status::Ok,
            "registry",
            match outcome {
                FetchOutcome::Updated => "index refreshed",
                FetchOutcome::NotModified => "index already current",
            },
        );
    }
    install::run_plan(plan, dry_run, paths, app_config).await
}
