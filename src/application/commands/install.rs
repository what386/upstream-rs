use anyhow::Result;
use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::operations::install_op::{InstallOperation, PlannedReleaseInstallRequest},
    application::{commands::build, context::CommandContext},
    models::{
        common::enums::{Channel, Filetype, Provider, TrustMode},
        upstream::{
            HttpInstallSource, InstallPlan, InstallSource, Package, ReleaseInstallSource,
            ReleaseSelector, config::AppConfig,
        },
    },
    output::{self, Status, TransactionRow},
    providers::{
        discovery::{
            DiscoveryRequest, DiscoveryResult, SourceKind, infer_package_name, infer_source,
            normalize_source_for_provider,
        },
        provider_manager::ProviderManager,
    },
    services::packaging::PackageProgressEvent,
    utils::static_paths::UpstreamPaths,
};

const INSTALL_PROGRESS_BAR_WIDTH: usize = 14;
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
        PackageProgressEvent::Detail(message) => {
            format!(" {:<28} {}", name, message)
        }
        PackageProgressEvent::Download { downloaded, total } => {
            let detail = if total > 0 {
                format!(
                    "Downloading {} {}",
                    output::progress_bar(downloaded, total, INSTALL_PROGRESS_BAR_WIDTH),
                    format_transfer(downloaded, total)
                )
            } else if downloaded > 0 {
                format!("Downloading {}", format_transfer(downloaded, total))
            } else {
                "Downloading...".to_string()
            };
            format!(" {:<28} {}", name, detail)
        }
        PackageProgressEvent::Zsync { downloaded, total } => {
            let detail = if total > 0 {
                format!(
                    "Zsync upgrading {} {}",
                    output::progress_bar(downloaded, total, INSTALL_PROGRESS_BAR_WIDTH),
                    format_transfer(downloaded, total)
                )
            } else if downloaded > 0 {
                format!("Zsync upgrading {}", format_transfer(downloaded, total))
            } else {
                "Zsync upgrading...".to_string()
            };
            format!(" {:<28} {}", name, detail)
        }
        PackageProgressEvent::Checksum { checked, total } => {
            let detail = if total > 0 {
                format!(
                    "Checksumming {} {}",
                    output::progress_bar(checked, total, INSTALL_PROGRESS_BAR_WIDTH),
                    format_transfer(checked, total)
                )
            } else if checked > 0 {
                format!("Checksumming {}", format_transfer(checked, total))
            } else {
                "Checksumming...".to_string()
            };
            format!(" {:<28} {}", name, detail)
        }
        PackageProgressEvent::Warning(message) => {
            format!(" {:<28} {}", name, message)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    name: Option<String>,
    repo_slug: String,
    kind: Filetype,
    version: Option<String>,
    semver: Option<String>,
    provider: Option<Provider>,
    base_url: Option<String>,
    channel: Channel,
    match_pattern: Option<String>,
    exclude_pattern: Option<String>,
    create_entry: bool,
    trust_mode: TrustMode,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let name = resolve_package_name(name, &repo_slug, provider.as_ref(), base_url.as_deref())?;
    let plan = InstallPlan {
        name,
        desktop: create_entry,
        source: InstallSource::Release(ReleaseInstallSource {
            source: repo_slug,
            kind,
            provider,
            base_url,
            channel,
            selector: ReleaseSelector::from_options(version, semver),
            match_pattern,
            exclude_pattern,
            trust_mode,
        }),
    };
    run_plan(plan, dry_run, paths, app_config).await
}

pub async fn run_plan(
    plan: InstallPlan,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let InstallPlan {
        name,
        desktop,
        source,
    } = plan;
    match source {
        InstallSource::Build(source) => {
            build::run_plan(
                InstallPlan {
                    name,
                    desktop,
                    source: InstallSource::Build(source),
                },
                dry_run,
                paths,
                app_config,
            )
            .await
        }
        InstallSource::Http(HttpInstallSource {
            url,
            kind,
            trust_mode,
        }) => {
            run_release_plan(
                name,
                desktop,
                ReleaseInstallSource {
                    source: url,
                    kind,
                    provider: Some(Provider::Direct),
                    base_url: None,
                    channel: Channel::Stable,
                    selector: ReleaseSelector::Latest,
                    match_pattern: None,
                    exclude_pattern: None,
                    trust_mode,
                },
                dry_run,
                paths,
                app_config,
            )
            .await
        }
        InstallSource::Release(source) => {
            run_release_plan(name, desktop, source, dry_run, paths, app_config).await
        }
    }
}

async fn run_release_plan(
    name: String,
    create_entry: bool,
    source: ReleaseInstallSource,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let (version, semver) = source.selector.into_options();
    let trust_mode = source.trust_mode;
    let context = CommandContext::new(paths, app_config)?;
    let mut package_database = context.package_database()?;
    let trusted_keys = context.trusted_keys()?;
    let package = build_package(
        &context.provider_manager,
        name,
        source.source,
        source.kind,
        source.provider,
        source.base_url,
        source.channel,
        source.match_pattern,
        source.exclude_pattern,
    )
    .await?;

    let mut install_operation = InstallOperation::new(
        &context.provider_manager,
        &mut package_database,
        context.paths,
        trusted_keys,
    )?;

    let preview = install_operation
        .preview_release_install(&package, &version, &semver)
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
        if should_emit
            || !matches!(
                event,
                PackageProgressEvent::Download { .. } | PackageProgressEvent::Zsync { .. }
            )
        {
            progress_pb.set_message(render_install_progress_message(&progress_name, event));
            last_emit = Some(std::time::Instant::now());
        }
    });

    let mut no_download_progress: Option<fn(u64, u64)> = None;
    let mut ignored_messages = Some(|_: &str| {});

    let install_result = install_operation
        .install_release_plan(
            PlannedReleaseInstallRequest {
                package,
                plan: preview,
                add_entry: create_entry,
                trust_mode,
            },
            &mut no_download_progress,
            &mut ignored_messages,
            &mut progress_callback,
        )
        .await;

    pb.finish_and_clear();

    match install_result {
        Ok(_) => {
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
                output::status_line_text(Status::Fail, &install_name, output::error_summary(&err))
            );
            println!(
                "{}",
                output::warning("Install complete: 0 installed, 1 failed.")
            );
            return Err(err.context(format!("Failed to install '{install_name}'")));
        }
    }

    Ok(())
}

fn resolve_package_name(
    name: Option<String>,
    source: &str,
    provider: Option<&Provider>,
    base_url: Option<&str>,
) -> Result<String> {
    if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
        return Ok(name);
    }

    let Some(default) = infer_package_name(source, provider, base_url)? else {
        return Err(anyhow::anyhow!(
            "Package name is required for this source. Provide a name after the repository or URL."
        ));
    };

    output::prompt_text("Package name", Some(&default))
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
    use super::{render_install_progress_message, render_install_progress_row};
    use crate::models::common::enums::Provider;
    use crate::providers::discovery::infer_package_name;
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
        let download = render_install_progress_row(
            "pnpm",
            PackageProgressEvent::Download {
                downloaded: 1024,
                total: 2048,
            },
        );
        assert!(download.starts_with(" pnpm                         Downloading [=======>      ]"));
        assert!(download.contains('/'));
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
    fn default_package_name_infers_git_repo_name_when_omitted() {
        assert_eq!(
            default_package_name("BurntSushi/ripgrep", None, None).expect("default name"),
            Some("ripgrep".to_string())
        );
        assert_eq!(
            default_package_name(
                "https://gitlab.example.com/group/project",
                Some(&Provider::Gitlab),
                Some("https://gitlab.example.com"),
            )
            .expect("default name"),
            Some("project".to_string())
        );
    }

    #[test]
    fn default_package_name_returns_none_for_http_sources() {
        let default =
            default_package_name("https://example.invalid/downloads", None, None).expect("default");

        assert_eq!(default, None);
    }

    fn default_package_name(
        source: &str,
        provider: Option<&Provider>,
        base_url: Option<&str>,
    ) -> anyhow::Result<Option<String>> {
        infer_package_name(source, provider, base_url)
    }
}
