use anyhow::Result;
use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::operations::install_operation::InstallOperation,
    application::output::{self, Status, TransactionRow},
    models::{
        common::enums::{Channel, Filetype, Provider, TrustMode},
        upstream::Package,
    },
    providers::{
        discovery::{
            DiscoveryRequest, DiscoveryResult, SourceKind, infer_source,
            normalize_source_for_provider,
        },
        provider_manager::ProviderManager,
    },
    services::{
        packaging::{PackagePhase, PackageProgressEvent},
        storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    },
    utils::static_paths::UpstreamPaths,
};

const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

fn format_transfer(downloaded: u64, total: u64) -> String {
    if total > 0 {
        format!("{} / {}", HumanBytes(downloaded), HumanBytes(total))
    } else if downloaded > 0 {
        format!("{}", HumanBytes(downloaded))
    } else {
        "-".to_string()
    }
}

fn format_error_chain(err: &anyhow::Error, max: usize) -> String {
    let mut parts = err
        .chain()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>();
    if parts.len() > 1
        && parts
            .first()
            .is_some_and(|part| part.starts_with("Failed to perform installation for "))
    {
        parts.remove(0);
    }
    parts.dedup();

    let value = parts.join(": ");
    if value.chars().count() <= max {
        return value;
    }

    let mut out = String::new();
    for ch in value.chars().take(max.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn render_install_progress_message(name: &str, event: PackageProgressEvent) -> String {
    format!(
        "Installing {name}\n{}",
        render_install_progress_row(name, event)
    )
}

fn render_install_progress_row(name: &str, event: PackageProgressEvent) -> String {
    match event {
        PackageProgressEvent::Phase(phase) => {
            format!(" {:<28} {}", name, phase.label())
        }
        PackageProgressEvent::Download { downloaded, total } => {
            format!(
                " {:<28} {:<28} {}",
                name,
                PackagePhase::DownloadingPackage.label(),
                format_transfer(downloaded, total)
            )
        }
        PackageProgressEvent::Warning(message) => {
            format!(" {:<28} {}", name, message)
        }
    }
}

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
    trust_mode: TrustMode,
    dry_run: bool,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;

    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let app_config = config.get_config();

    let github_token = app_config.github.api_token.as_deref();
    let gitlab_token = app_config.gitlab.api_token.as_deref();
    let gitea_token = app_config.gitea.api_token.as_deref();

    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;
    let trusted_keys = app_config.trusted_signature_keys();

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
    )
    .await?;

    let mut package_installer = InstallOperation::new(
        &provider_manager,
        &mut package_storage,
        &paths,
        trusted_keys,
    )?;

    let preview = package_installer
        .preview_single_install(&package, &version)
        .await?;

    if dry_run {
        println!("{}", output::title("Install preview"));
        output::kv("Package", &package.name);
        output::kv(
            "Source",
            format!("{} ({})", package.repo_slug, package.provider),
        );
        output::kv(
            "Release",
            format!("{} ({})", preview.release_name, preview.release_tag),
        );
        output::kv(
            "Asset",
            format!("{} ({:?})", preview.asset_name, preview.resolved_filetype),
        );
        output::kv("Trust", trust_mode);
        output::kv("Desktop", if create_entry { "yes" } else { "no" });
        output::print_disk_impact(&preview.disk_impact, true);
        output::action_note("resolve only (no download, no install, no metadata changes)");
        return Ok(());
    }

    let transaction_rows = vec![TransactionRow::single_version(
        format!("{}/{}", package.provider, package.name),
        &preview.release_tag,
        preview.disk_impact.net,
        preview.disk_impact.download,
    )];
    output::print_transaction_table(&transaction_rows, &preview.disk_impact, "Net disk change:");
    output::confirm_or_cancel("Proceed with installation?", true)?;

    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message(format!("Installing {}", package.name));

    let install_name = package.name.clone();
    let progress_name = install_name.clone();
    let install_version = preview.release_tag.clone();
    let progress_pb = pb.clone();
    let mut last_emit = None;
    let mut progress_callback = Some(move |event: PackageProgressEvent| {
        let should_emit = last_emit
            .map(|elapsed: std::time::Instant| elapsed.elapsed() >= PROGRESS_UPDATE_INTERVAL)
            .unwrap_or(true);
        if should_emit || !matches!(event, PackageProgressEvent::Download { .. }) {
            progress_pb.set_message(render_install_progress_message(&progress_name, event));
            last_emit = Some(std::time::Instant::now());
        }
    });

    let mut no_download_progress: Option<fn(u64, u64)> = None;
    let mut ignored_messages = Some(|_: &str| {});

    let install_result = package_installer
        .install_single_with_progress(
            package,
            &version,
            &create_entry,
            trust_mode,
            &mut no_download_progress,
            &mut ignored_messages,
            &mut progress_callback,
        )
        .await;

    pb.finish_and_clear();

    match install_result {
        Ok(()) => {
            println!(
                "{}",
                output::status_line_text(
                    Status::Ok,
                    &install_name,
                    format!("installed {install_version}")
                )
            );
            println!(
                "{}",
                output::success("Install complete: 1 installed, 0 failed.")
            );
        }
        Err(err) => {
            println!(
                "{}",
                output::status_line_text(
                    Status::Fail,
                    &install_name,
                    format_error_chain(&err, 160)
                )
            );
            println!(
                "{}",
                output::warning("Install complete: 0 installed, 1 failed.")
            );
        }
    }

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
) -> Result<Package> {
    let Some(provider) = provider else {
        let mut source_info = infer_source(&source)?;
        if let Some(base_url) = base_url.clone() {
            source_info.base_url = Some(base_url);
        }

        if !matches!(source_info.kind, SourceKind::DownloadPage) {
            println!(
                "{}",
                output::title(format!(
                    "Discovered source: {} via {}",
                    source_info.repo_slug, source_info.provider
                ))
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
        confirm_discovery_if_needed(&discovery)?;

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
        output::title(format!(
            "Discovered source: {} via {}",
            discovery.source.repo_slug, discovery.source.provider
        ))
    );

    let should_show_candidates = matches!(discovery.source.kind, SourceKind::DownloadPage)
        || discovery.is_ambiguous()
        || discovery.recommended_candidate().is_some();

    if !should_show_candidates {
        return;
    }

    println!("{}", output::section("Top discovered assets:"));
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

fn confirm_discovery_if_needed(discovery: &DiscoveryResult) -> Result<()> {
    if output::assume_yes()
        || !matches!(discovery.source.kind, SourceKind::DownloadPage)
        || !discovery.is_ambiguous()
    {
        return Ok(());
    }

    let Some(candidate) = discovery.recommended_candidate() else {
        return Ok(());
    };

    output::confirm_or_cancel(
        format!(
            "Install recommended asset '{}' from this page?",
            candidate.asset.name
        ),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::{format_error_chain, render_install_progress_message, render_install_progress_row};
    use crate::services::packaging::{PackagePhase, PackageProgressEvent};

    #[test]
    fn install_progress_row_renders_phase_warning_and_download() {
        assert_eq!(
            render_install_progress_row(
                "pnpm",
                PackageProgressEvent::Phase(PackagePhase::VerifyingSignature)
            ),
            " pnpm                         Verifying signature ..."
        );
        assert_eq!(
            render_install_progress_row(
                "pnpm",
                PackageProgressEvent::Warning("Completion install skipped".to_string())
            ),
            " pnpm                         Completion install skipped"
        );
        assert!(
            render_install_progress_row(
                "pnpm",
                PackageProgressEvent::Download {
                    downloaded: 1024,
                    total: 2048,
                },
            )
            .contains('/')
        );
    }

    #[test]
    fn install_progress_message_keeps_phase_inside_spinner_message() {
        assert_eq!(
            render_install_progress_message(
                "pnpm",
                PackageProgressEvent::Phase(PackagePhase::InstallingPackage)
            ),
            "Installing pnpm\n pnpm                         Installing package ..."
        );
    }

    #[test]
    fn install_error_chain_removes_outer_install_wrapper() {
        let err = anyhow::anyhow!("signature key missing")
            .context("Failed trust verification")
            .context("Failed to perform installation for 'pnpm'");

        let formatted = format_error_chain(&err, 160);

        assert!(!formatted.contains("Failed to perform installation for 'pnpm'"));
        assert!(formatted.contains("Failed trust verification"));
        assert!(formatted.contains("signature key missing"));
    }
}
