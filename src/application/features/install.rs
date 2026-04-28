use anyhow::{Result, anyhow};
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use crate::{
    application::operations::install_operation::InstallOperation,
    models::{
        common::enums::{Channel, Filetype, Provider},
        upstream::Package,
    },
    providers::{
        discovery::{
            DiscoveryRequest, DiscoveryResult, SourceKind, infer_source, normalize_source_for_provider,
        },
        provider_manager::ProviderManager,
    },
    services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};

#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: String,
    repo_slug: String,
    kind: Filetype,
    version: Option<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    match_pattern: Option<String>,
    exclude_pattern: Option<String>,
    create_entry: bool,
    ignore_checksums: bool,
    yes: bool,
) -> Result<()> {
    const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

    let paths = UpstreamPaths::new()?;

    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let github_token = config.get_config().github.api_token.as_deref();
    let gitlab_token = config.get_config().gitlab.api_token.as_deref();
    let gitea_token = config.get_config().gitea.api_token.as_deref();

    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;

    let package = build_package(
        &provider_manager,
        name,
        repo_slug,
        kind,
        provider,
        base_url,
        channel,
        match_pattern,
        exclude_pattern,
        yes,
    )
    .await?;

    println!(
        "{}",
        style(format!(
            "Installing {} from {} ...",
            &package.name, &package.provider
        ))
        .cyan()
    );

    let mut package_installer =
        InstallOperation::new(&provider_manager, &mut package_storage, &paths)?;

    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}",
    )?);
    pb.enable_steady_tick(Duration::from_millis(120));

    // Borrow pb for the closures
    let pb_ref = &pb;
    let mut last_emit: Option<Instant> = None;
    let mut last_progress: Option<(u64, u64)> = None;
    let mut download_progress_callback = Some(|downloaded: u64, total: u64| {
        last_progress = Some((downloaded, total));
        let should_emit = last_emit
            .map(|t| t.elapsed() >= PROGRESS_UPDATE_INTERVAL)
            .unwrap_or(true);
        if should_emit {
            pb_ref.set_length(total);
            pb_ref.set_position(downloaded);
            last_emit = Some(Instant::now());
        }
    });

    let mut message_callback = Some(move |msg: &str| {
        pb_ref.println(msg);
    });

    package_installer
        .install_single(
            package,
            &version,
            &create_entry,
            ignore_checksums,
            &mut download_progress_callback,
            &mut message_callback,
        )
        .await?;

    if let Some((downloaded, total)) = last_progress {
        pb.set_length(total);
        pb.set_position(downloaded);
    }

    // Set pb to 100%
    pb.set_position(pb.length().unwrap_or(0));

    pb.finish_with_message("Install complete");
    println!("{}", style("Install complete.").green());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn build_package(
    provider_manager: &ProviderManager,
    name: String,
    source: String,
    kind: Filetype,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    match_pattern: Option<String>,
    exclude_pattern: Option<String>,
    yes: bool,
) -> Result<Package> {
    let Some(provider) = provider else {
        let mut source_info = infer_source(&source)?;
        if let Some(base_url) = base_url.clone() {
            source_info.base_url = Some(base_url);
        }

        if !matches!(source_info.kind, SourceKind::DownloadPage) {
            println!(
                "{}",
                style(format!(
                    "Discovered source: {} via {}",
                    source_info.repo_slug, source_info.provider
                ))
                .cyan()
            );
            return Ok(Package::with_defaults(
                name,
                source_info.repo_slug,
                kind,
                match_pattern,
                exclude_pattern,
                channel,
                source_info.provider,
                source_info.base_url,
            ));
        }

        let discovery = provider_manager
            .discover_source(DiscoveryRequest {
                source,
                channel: channel.clone(),
                package_name: name.clone(),
                filetype: kind,
                match_pattern: match_pattern.clone(),
                exclude_pattern: exclude_pattern.clone(),
                base_url_override: base_url.clone(),
                limit: 10,
            })
            .await?;

        render_discovery_summary(&discovery);
        confirm_discovery_if_needed(&discovery, yes)?;

        return Ok(Package::with_defaults(
            name,
            discovery.source.repo_slug,
            kind,
            match_pattern,
            exclude_pattern,
            channel,
            discovery.source.provider,
            discovery.source.base_url,
        ));
    };

    let normalized_source = normalize_source_for_provider(&source, &provider, base_url.as_deref());

    Ok(Package::with_defaults(
        name,
        normalized_source,
        kind,
        match_pattern,
        exclude_pattern,
        channel,
        provider,
        base_url,
    ))
}

fn render_discovery_summary(discovery: &DiscoveryResult) {
    println!(
        "{}",
        style(format!(
            "Discovered source: {} via {}",
            discovery.source.repo_slug, discovery.source.provider
        ))
        .cyan()
    );

    let should_show_candidates = matches!(discovery.source.kind, SourceKind::DownloadPage)
        || discovery.is_ambiguous()
        || discovery.recommended_candidate().is_some();

    if !should_show_candidates {
        return;
    }

    println!("{}", style("Top discovered assets:").bold());
    for (idx, candidate) in discovery.candidates.iter().take(5).enumerate() {
        println!(
            "  {}. {} ({:?}, score={})",
            idx + 1,
            candidate.asset.name,
            candidate.asset.filetype,
            candidate.score
        );
    }
}

fn confirm_discovery_if_needed(discovery: &DiscoveryResult, yes: bool) -> Result<()> {
    if yes
        || !matches!(discovery.source.kind, SourceKind::DownloadPage)
        || !discovery.is_ambiguous()
    {
        return Ok(());
    }

    let Some(candidate) = discovery.recommended_candidate() else {
        return Ok(());
    };

    if !io::stdin().is_terminal() {
        return Err(anyhow!(
            "Discovery found multiple plausible assets for '{}'. Re-run with --yes to accept '{}' or use --match/--exclude to narrow the choice.",
            discovery.source.original,
            candidate.asset.name
        ));
    }

    print!(
        "Install recommended asset '{}' from this page? [Y/N]: ",
        candidate.asset.name
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase().starts_with("y") {
        Ok(())
    } else {
        Err(anyhow!("Install cancelled"))
    }
}
