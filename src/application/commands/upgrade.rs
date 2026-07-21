use crate::{
    application::commands::changelog::changelog_text_for_package,
    application::context::CommandContext,
    application::operations::history_op,
    application::operations::upgrade_op::{
        UpdateCheckRow, UpdateCheckStatus, UpgradeOperation, UpgradePackageResult,
        UpgradePreviewEvent, UpgradeProgressEvent,
    },
    models::{common::enums::TrustMode, upstream::config::AppConfig},
    output::{self, SizeImpactRow, Status, TransactionRow, TransactionTableLayout},
    providers::provider_manager::ProviderManager,
    services::packaging::{PackagePhase, PackageProgressEvent},
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal, Write},
    time::Duration,
};

const UPGRADE_PROGRESS_BAR_WIDTH: usize = 14;

fn new_check_progress_bar() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stdout_with_hz(10));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}").expect("valid progress template"),
    );
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("checking for updates");
    pb
}

fn upgrade_transaction_row(
    row: &crate::application::operations::upgrade_op::UpgradePreviewRow,
) -> TransactionRow {
    TransactionRow::new(
        format!("{}/{}", row.source, row.name),
        &row.old_version,
        &row.new_version,
        row.disk_impact.net,
        row.disk_impact.download,
    )
}

fn render_upgrade_progress(
    active_rows: &BTreeMap<String, String>,
    completed: u32,
    total: u32,
) -> String {
    let active_count = active_rows.len() as u32;
    let queued = total.saturating_sub(completed).saturating_sub(active_count);
    let mut parts = vec![format!(" ({queued} queued)")];
    if active_rows.is_empty() {
        return parts.join("");
    }

    let rows = active_rows.values().cloned().collect::<Vec<_>>();
    parts.push(format!("\n{}", rows.join("\n")));
    parts.join("")
}

fn phase_label_for_progress(phase: PackagePhase) -> String {
    phase.label().replace(" ...", "...")
}

fn render_upgrade_progress_row(
    name: &str,
    event: PackageProgressEvent,
    name_width: usize,
) -> String {
    let detail = match event {
        PackageProgressEvent::Phase(phase) => phase_label_for_progress(phase),
        PackageProgressEvent::Detail(message) => output::truncate_end(&message, 96),
        PackageProgressEvent::Download { downloaded, total } if total > 0 => {
            format!(
                "Downloading {} {} / {}",
                output::progress_bar(downloaded, total, UPGRADE_PROGRESS_BAR_WIDTH),
                HumanBytes(downloaded),
                HumanBytes(total)
            )
        }
        PackageProgressEvent::Download { downloaded, .. } if downloaded > 0 => {
            format!("Downloading {}", HumanBytes(downloaded))
        }
        PackageProgressEvent::Download { .. } => "Downloading...".to_string(),
        PackageProgressEvent::Zsync { downloaded, total } if total > 0 => {
            format!(
                "Zsync upgrading {} {} / {}",
                output::progress_bar(downloaded, total, UPGRADE_PROGRESS_BAR_WIDTH),
                HumanBytes(downloaded),
                HumanBytes(total)
            )
        }
        PackageProgressEvent::Zsync { downloaded, .. } if downloaded > 0 => {
            format!("Zsync upgrading {}", HumanBytes(downloaded))
        }
        PackageProgressEvent::Zsync { .. } => "Zsync upgrading...".to_string(),
        PackageProgressEvent::Checksum { checked, total } if total > 0 => {
            format!(
                "Checksumming {} {} / {}",
                output::progress_bar(checked, total, UPGRADE_PROGRESS_BAR_WIDTH),
                HumanBytes(checked),
                HumanBytes(total)
            )
        }
        PackageProgressEvent::Checksum { checked, .. } if checked > 0 => {
            format!("Checksumming {}", HumanBytes(checked))
        }
        PackageProgressEvent::Checksum { .. } => "Checksumming...".to_string(),
        PackageProgressEvent::Warning(message) => output::truncate_end(&message, 96),
    };
    format!("{name:<name_width$} {detail}")
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    names: Option<Vec<String>>,
    force_option: bool,
    check_option: bool,
    machine_readable: bool,
    json: bool,
    trust_mode: TrustMode,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    let context = CommandContext::new(paths, app_config)?;
    let trusted_keys = context.trusted_keys()?;
    let mut package_database = context.package_database()?;
    let mut package_upgrade = UpgradeOperation::new(
        &context.provider_manager,
        &mut package_database,
        context.paths,
        trusted_keys,
        context.app_config.concurrency,
    )?;

    // Handle --check flag
    if check_option {
        return run_check(package_upgrade, names, machine_readable, json).await;
    }
    if dry_run {
        return run_dry_run(package_upgrade, names, force_option, trust_mode).await;
    }

    let mut live_layout: Option<TransactionTableLayout> = None;
    let mut check_pb: Option<ProgressBar> = None;
    let mut printed_live_row = false;
    let preview_result = package_upgrade
        .preview_upgrade(names.as_deref(), force_option, &mut |event| match event {
            UpgradePreviewEvent::Started { package_width } => {
                let layout = TransactionTableLayout::upgrade_preview(package_width);
                live_layout = Some(layout);

                check_pb = Some(new_check_progress_bar());
            }
            UpgradePreviewEvent::Checking { name } => {
                if let Some(pb) = &check_pb {
                    pb.set_message(format!("checking for updates: {name}"));
                }
            }
            UpgradePreviewEvent::Row(row) => {
                if let Some(layout) = &live_layout {
                    if let Some(pb) = &check_pb {
                        pb.suspend(|| {
                            if !printed_live_row {
                                layout.print_header();
                            }
                            layout.print_row(&upgrade_transaction_row(&row));
                        });
                    } else {
                        if !printed_live_row {
                            layout.print_header();
                        }
                        layout.print_row(&upgrade_transaction_row(&row));
                    }
                    printed_live_row = true;
                }
            }
        })
        .await;
    let preview_rows = preview_result?;
    if let Some(pb) = &check_pb {
        pb.finish_and_clear();
    }
    let impact = preview_rows.iter().fold(
        crate::services::packaging::disk_impact::DiskImpact::empty(),
        |total, row| total + row.disk_impact.clone(),
    );
    if let Some(layout) = &live_layout {
        if !printed_live_row {
            println!("No upgrades available.");
            return Ok(());
        }
        if preview_rows.iter().all(|row| row.source_build) {
            println!();
        } else {
            let rollback_impact = package_upgrade.estimate_upgrade_rollback_impact(&preview_rows);
            let size_rows = rollback_size_rows(rollback_impact);
            layout.print_totals(&impact, "Net disk change:", &size_rows);
        }
    } else {
        let transaction_rows = preview_rows
            .iter()
            .map(upgrade_transaction_row)
            .collect::<Vec<_>>();
        if preview_rows.iter().all(|row| row.source_build) {
            output::print_transaction_table_without_size(&transaction_rows);
        } else {
            let rollback_impact = package_upgrade.estimate_upgrade_rollback_impact(&preview_rows);
            let size_rows = rollback_size_rows(rollback_impact);
            output::print_transaction_table_with_size_rows(
                &transaction_rows,
                &impact,
                "Net disk change:",
                &size_rows,
            );
        }
    }
    confirm_or_show_changelog(&context.provider_manager, &preview_rows).await?;

    let overall_pb = ProgressBar::new(preview_rows.len() as u64);
    overall_pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    overall_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Upgraded {pos}/{len} packages{msg}",
    )?);
    overall_pb.enable_steady_tick(Duration::from_millis(120));

    let progress_pb = overall_pb.clone();
    let mut active_progress_rows = BTreeMap::new();
    let completion_subject_width =
        output::status_subject_width(preview_rows.iter().map(|row| row.name.as_str()));
    let active_name_width = preview_rows
        .iter()
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(0);
    let mut completed_count = 0_u32;
    let mut total_count = preview_rows.len() as u32;
    progress_pb.set_message(render_upgrade_progress(
        &active_progress_rows,
        completed_count,
        total_count,
    ));
    let mut progress_callback = Some(|event: UpgradeProgressEvent| {
        match event {
            UpgradeProgressEvent::Overall { completed, total } => {
                completed_count = completed;
                total_count = total;
                progress_pb.set_length(total as u64);
                progress_pb.set_position(completed as u64);
            }
            UpgradeProgressEvent::Package { name, event } => {
                active_progress_rows.insert(
                    name.clone(),
                    render_upgrade_progress_row(&name, event, active_name_width),
                );
            }
            UpgradeProgressEvent::Warning { name, message } => {
                let row = output::status_line_text_with_width(
                    Status::Warn,
                    &name,
                    message,
                    completion_subject_width,
                );
                progress_pb.suspend(|| println!("{row}"));
            }
            UpgradeProgressEvent::Complete { name, result } => {
                active_progress_rows.remove(&name);
                let row = match result {
                    UpgradePackageResult::Upgraded { version } => {
                        let row = output::status_line_text_with_width(
                            Status::Ok,
                            &name,
                            format!("upgraded to {version}"),
                            completion_subject_width,
                        );
                        if let Some(preview) = preview_rows.iter().find(|row| row.name == name) {
                            history_op::record_version_item(
                                name.clone(),
                                preview.old_version.clone(),
                                version,
                            );
                        }
                        row
                    }
                    UpgradePackageResult::Failed { error } => output::status_line_text_with_width(
                        Status::Fail,
                        &name,
                        error,
                        completion_subject_width,
                    ),
                };
                progress_pb.suspend(|| println!("{row}"));
            }
            UpgradeProgressEvent::Clear => {
                active_progress_rows.clear();
            }
        }
        progress_pb.set_message(render_upgrade_progress(
            &active_progress_rows,
            completed_count,
            total_count,
        ));
    });
    let bulk_result = package_upgrade
        .upgrade_resolved_bulk(&preview_rows, trust_mode, &mut progress_callback)
        .await;
    let (upgraded, failed) = match bulk_result {
        Ok(result) => result,
        Err(err) => {
            overall_pb.finish_and_clear();
            return Err(err);
        }
    };

    overall_pb.finish_and_clear();
    if failed > 0 {
        println!(
            "{}",
            output::warning(format!(
                "Upgrade complete: {} upgraded, {} failed.",
                upgraded, failed
            ))
        );
    } else {
        println!(
            "{}",
            output::success(format!(
                "Upgrade complete: {} upgraded, 0 failed.",
                upgraded
            ))
        );
    }

    Ok(())
}

enum UpgradePromptAction {
    Proceed,
    Changelog,
}

fn upgrade_prompt_action_from_input(input: &str) -> Result<UpgradePromptAction> {
    match input.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Ok(UpgradePromptAction::Proceed),
        "c" | "changelog" => Ok(UpgradePromptAction::Changelog),
        _ => anyhow::bail!("Cancelled"),
    }
}

fn prompt_upgrade_action() -> Result<UpgradePromptAction> {
    if output::assume_yes() {
        return Ok(UpgradePromptAction::Proceed);
    }

    if !io::stdin().is_terminal() {
        anyhow::bail!(
            "Confirmation required for non-interactive input. Re-run with --yes to continue."
        );
    }

    print!("Proceed with installation? [Y/n/c] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    upgrade_prompt_action_from_input(&input)
}

async fn confirm_or_show_changelog(
    provider_manager: &ProviderManager,
    preview_rows: &[crate::application::operations::upgrade_op::UpgradePreviewRow],
) -> Result<()> {
    loop {
        match prompt_upgrade_action()? {
            UpgradePromptAction::Proceed => return Ok(()),
            UpgradePromptAction::Changelog => {
                show_upgrade_changelog(provider_manager, preview_rows).await?;
            }
        }
    }
}

async fn show_upgrade_changelog(
    provider_manager: &ProviderManager,
    preview_rows: &[crate::application::operations::upgrade_op::UpgradePreviewRow],
) -> Result<()> {
    let mut sections = Vec::new();

    for row in preview_rows {
        let crate::services::packaging::ResolvedUpgradeTarget::Release(release) = &row.target
        else {
            continue;
        };
        output::action_note(format!("fetching changelog for {}", row.name));
        match changelog_text_for_package(
            provider_manager,
            &row.package,
            &row.package.version,
            row.package.last_upgraded,
            release,
            false,
        )
        .await
        {
            Ok(Some(text)) => {
                sections.push(format!("# {}\n\n{}", row.name, text));
            }
            Ok(None) => {}
            Err(error) => {
                println!(
                    "{}",
                    output::warning(format!(
                        "Failed to fetch changelog for {}: {error}",
                        row.name
                    ))
                );
            }
        }
    }

    if sections.is_empty() {
        println!(
            "{}",
            output::warning("No release changelog is available for the planned upgrade(s).")
        );
        return Ok(());
    }

    output::pager::page_text(Some("Upgrade changelog"), &sections.join("\n"))?;
    Ok(())
}

fn rollback_size_rows(
    rollback_impact: crate::services::packaging::disk_impact::SignedByteEstimate,
) -> Vec<SizeImpactRow> {
    if matches!(rollback_impact.bytes, Some(0)) {
        Vec::new()
    } else {
        vec![SizeImpactRow::new("Rollback storage", rollback_impact)]
    }
}

fn truncate_cell(value: &str, max: usize) -> String {
    output::truncate_end(value, max)
}

#[derive(Debug, Clone, Copy)]
struct CheckTableLayout {
    name: usize,
    channel: usize,
    source: usize,
}

impl CheckTableLayout {
    fn from_rows(rows: &[&UpdateCheckRow]) -> Self {
        let name = rows
            .iter()
            .map(|row| row.name.chars().count())
            .max()
            .unwrap_or("Name".len())
            .clamp("Name".len(), 18);
        let channel = rows
            .iter()
            .map(|row| {
                row.channel
                    .as_ref()
                    .map(|c| c.to_string().to_lowercase().chars().count())
                    .unwrap_or(1)
            })
            .max()
            .unwrap_or("Channel".len())
            .clamp("Channel".len(), 7);
        let source = rows
            .iter()
            .map(|row| {
                row.provider
                    .as_ref()
                    .map(|provider| provider.to_string().chars().count())
                    .unwrap_or(1)
            })
            .max()
            .unwrap_or("Source".len())
            .clamp("Source".len(), 7);

        Self {
            name,
            channel,
            source,
        }
    }

    fn all_rows(rows: &[UpdateCheckRow]) -> Self {
        let display_rows = rows.iter().collect::<Vec<_>>();
        Self::from_rows(&display_rows)
    }
}

fn render_check_table(rows: &[UpdateCheckRow]) {
    if rows.is_empty() {
        println!("No installed packages to check.");
        return;
    }

    let mut available = 0_u32;
    let mut up_to_date = 0_u32;
    let mut failed = 0_u32;
    let mut not_installed = 0_u32;
    let mut display_rows: Vec<&UpdateCheckRow> = Vec::new();

    for row in rows {
        match &row.status {
            UpdateCheckStatus::UpdateAvailable { current, latest } => {
                available += 1;
                let _ = (current, latest);
                display_rows.push(row);
            }
            UpdateCheckStatus::UpToDate { current } => {
                up_to_date += 1;
                let _ = current;
            }
            UpdateCheckStatus::Failed { error } => {
                failed += 1;
                let _ = error;
                display_rows.push(row);
            }
            UpdateCheckStatus::NotInstalled => {
                not_installed += 1;
                display_rows.push(row);
            }
        }
    }

    println!("{}", output::title("Checking for updates"));

    if !display_rows.is_empty() {
        let layout = CheckTableLayout::from_rows(&display_rows);
        println!();
        println!(
            "{}",
            output::section(format!(
                "{:<8} {:<name$} {:<channel$} {:<source$} Version",
                "State",
                "Name",
                "Channel",
                "Source",
                name = layout.name,
                channel = layout.channel,
                source = layout.source,
            ))
        );

        for row in &display_rows {
            let (status, version) = match &row.status {
                UpdateCheckStatus::UpdateAvailable { current, latest } => (
                    output::status_cell(Status::Plan).to_string(),
                    format!("{current} -> {latest}"),
                ),
                UpdateCheckStatus::Failed { error } => {
                    (output::status_cell(Status::Fail).to_string(), error.clone())
                }
                UpdateCheckStatus::NotInstalled => (
                    output::status_cell(Status::Fail).to_string(),
                    "not installed".to_string(),
                ),
                UpdateCheckStatus::UpToDate { .. } => continue,
            };

            let branch = row
                .channel
                .as_ref()
                .map(|c| c.to_string().to_lowercase())
                .unwrap_or_else(|| "-".to_string());
            let remote = row
                .provider
                .as_ref()
                .map(std::string::ToString::to_string)
                .unwrap_or_else(|| "-".to_string());

            println!(
                "{} {:<name$} {:<channel$} {:<source$} {}",
                status,
                truncate_cell(&row.name, layout.name),
                truncate_cell(&branch, layout.channel),
                truncate_cell(&remote, layout.source),
                version,
                name = layout.name,
                channel = layout.channel,
                source = layout.source,
            );
        }
    }

    let status = if failed > 0 || not_installed > 0 {
        Status::Warn
    } else {
        Status::Ok
    };
    if !display_rows.is_empty() {
        println!();
    }
    output::summary_line(
        status,
        format!(
            "{} available, {} up to date, {} failed, {} not installed",
            available, up_to_date, failed, not_installed
        ),
    );
}

#[derive(Serialize)]
struct JsonUpdateCheckRow {
    name: String,
    channel: Option<String>,
    provider: Option<String>,
    state: &'static str,
    current: Option<String>,
    latest: Option<String>,
    error: Option<String>,
}

fn json_check_rows(rows: Vec<UpdateCheckRow>) -> Vec<JsonUpdateCheckRow> {
    rows.into_iter()
        .map(|row| {
            let channel = row.channel.map(|channel| channel.to_string());
            let provider = row.provider.map(|provider| provider.to_string());
            match row.status {
                UpdateCheckStatus::UpdateAvailable { current, latest } => JsonUpdateCheckRow {
                    name: row.name,
                    channel,
                    provider,
                    state: "update_available",
                    current: Some(current),
                    latest: Some(latest),
                    error: None,
                },
                UpdateCheckStatus::UpToDate { current } => JsonUpdateCheckRow {
                    name: row.name,
                    channel,
                    provider,
                    state: "up_to_date",
                    current: Some(current),
                    latest: None,
                    error: None,
                },
                UpdateCheckStatus::Failed { error } => JsonUpdateCheckRow {
                    name: row.name,
                    channel,
                    provider,
                    state: "failed",
                    current: None,
                    latest: None,
                    error: Some(error),
                },
                UpdateCheckStatus::NotInstalled => JsonUpdateCheckRow {
                    name: row.name,
                    channel,
                    provider,
                    state: "not_installed",
                    current: None,
                    latest: None,
                    error: None,
                },
            }
        })
        .collect()
}

fn check_failure_count(rows: &[UpdateCheckRow]) -> usize {
    rows.iter()
        .filter(|row| {
            matches!(
                row.status,
                UpdateCheckStatus::Failed { .. } | UpdateCheckStatus::NotInstalled
            )
        })
        .count()
}

async fn run_check(
    package_upgrade: UpgradeOperation<'_>,
    names: Option<Vec<String>>,
    machine_readable: bool,
    json: bool,
) -> Result<()> {
    if json {
        let rows = package_upgrade
            .check_detailed(names.as_deref(), &mut |_| {})
            .await;
        let failed = check_failure_count(&rows);
        println!("{}", serde_json::to_string_pretty(&json_check_rows(rows))?);
        if failed > 0 {
            anyhow::bail!("{failed} update check(s) failed");
        }
    } else if machine_readable {
        let rows = package_upgrade
            .check_detailed(names.as_deref(), &mut |_| {})
            .await;
        let failed = check_failure_count(&rows);
        for row in &rows {
            if let UpdateCheckStatus::UpdateAvailable { current, latest } = &row.status {
                println!("{} {current} {latest}", row.name);
            }
        }
        if failed > 0 {
            anyhow::bail!("{failed} update check(s) failed");
        }
    } else {
        let check_pb = new_check_progress_bar();
        let mut checking_callback = |name: &str| {
            check_pb.set_message(format!("checking for updates: {name}"));
        };
        let rows = package_upgrade
            .check_detailed(names.as_deref(), &mut checking_callback)
            .await;
        check_pb.finish_and_clear();
        render_check_table(&rows);
        let failed = check_failure_count(&rows);
        if failed > 0 {
            anyhow::bail!("{failed} update check(s) failed");
        }
    }

    Ok(())
}

async fn run_dry_run(
    package_upgrade: UpgradeOperation<'_>,
    names: Option<Vec<String>>,
    force_option: bool,
    trust_mode: TrustMode,
) -> Result<()> {
    println!("{}", output::title("Upgrade preview"));
    output::kv("Trust", trust_mode);
    let preview_rows = package_upgrade
        .preview_upgrade(names.as_deref(), force_option, &mut |_| {})
        .await;
    let (impact, rollback_impact) = match &preview_rows {
        Ok(rows) => {
            let impact = rows.iter().fold(
                crate::services::packaging::disk_impact::DiskImpact::empty(),
                |total, row| total + row.disk_impact.clone(),
            );
            (
                impact,
                package_upgrade.estimate_upgrade_rollback_impact(rows),
            )
        }
        Err(_) => (
            crate::services::packaging::disk_impact::DiskImpact::unknown(),
            crate::services::packaging::disk_impact::SignedByteEstimate::unknown(),
        ),
    };
    let size_rows = rollback_size_rows(rollback_impact);
    output::print_disk_impact_with_size_rows(&impact, &size_rows, true);
    output::action_note("resolve only (no download, no install, no metadata changes)");
    println!();
    let rows = package_upgrade
        .check_detailed(names.as_deref(), &mut |_| {})
        .await;

    if rows.is_empty() {
        println!("{}", output::warning("No installed packages to check."));
        return Ok(());
    }

    let layout = CheckTableLayout::all_rows(&rows);
    println!(
        "{}",
        output::section(format!(
            "{:<8} {:<name$} {:<channel$} {:<source$} Plan",
            "State",
            "Name",
            "Channel",
            "Source",
            name = layout.name,
            channel = layout.channel,
            source = layout.source,
        ))
    );

    let mut would_upgrade = 0_u32;
    let mut up_to_date = 0_u32;
    let mut failed = 0_u32;
    let mut not_installed = 0_u32;

    for row in rows {
        let branch = row
            .channel
            .as_ref()
            .map(|c| c.to_string().to_lowercase())
            .unwrap_or_else(|| "-".to_string());
        let remote = row
            .provider
            .as_ref()
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "-".to_string());

        match row.status {
            UpdateCheckStatus::UpdateAvailable { current, latest } => {
                would_upgrade += 1;
                let plan = if force_option {
                    format!("would force-upgrade {current} -> {latest}")
                } else {
                    format!("would upgrade {current} -> {latest}")
                };
                println!(
                    "{} {:<name$} {:<channel$} {:<source$} {}",
                    output::status_cell(Status::Plan),
                    truncate_cell(&row.name, layout.name),
                    truncate_cell(&branch, layout.channel),
                    truncate_cell(&remote, layout.source),
                    plan,
                    name = layout.name,
                    channel = layout.channel,
                    source = layout.source,
                );
            }
            UpdateCheckStatus::UpToDate { current } => {
                if force_option {
                    would_upgrade += 1;
                    println!(
                        "{} {:<name$} {:<channel$} {:<source$} force-upgrade {}",
                        output::status_cell(Status::Plan),
                        truncate_cell(&row.name, layout.name),
                        truncate_cell(&branch, layout.channel),
                        truncate_cell(&remote, layout.source),
                        current,
                        name = layout.name,
                        channel = layout.channel,
                        source = layout.source,
                    );
                } else {
                    up_to_date += 1;
                    let _ = current;
                }
            }
            UpdateCheckStatus::Failed { error } => {
                failed += 1;
                println!(
                    "{} {:<name$} {:<channel$} {:<source$} failed: {}",
                    output::status_cell(Status::Fail),
                    truncate_cell(&row.name, layout.name),
                    truncate_cell(&branch, layout.channel),
                    truncate_cell(&remote, layout.source),
                    error,
                    name = layout.name,
                    channel = layout.channel,
                    source = layout.source,
                );
            }
            UpdateCheckStatus::NotInstalled => {
                not_installed += 1;
                println!(
                    "{} {:<name$} {:<channel$} {:<source$} not installed",
                    output::status_cell(Status::Fail),
                    truncate_cell(&row.name, layout.name),
                    truncate_cell(&branch, layout.channel),
                    truncate_cell(&remote, layout.source),
                    name = layout.name,
                    channel = layout.channel,
                    source = layout.source,
                );
            }
        }
    }

    println!();
    let status = if failed > 0 || not_installed > 0 {
        Status::Warn
    } else {
        Status::Ok
    };
    output::status_line(
        status,
        "summary",
        format!(
            "{} planned, {} up to date, {} failed, {} not installed",
            would_upgrade, up_to_date, failed, not_installed
        ),
    );
    if failed > 0 || not_installed > 0 {
        anyhow::bail!(
            "{} upgrade preview(s) failed",
            failed.saturating_add(not_installed)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CheckTableLayout, UpgradePromptAction, json_check_rows, render_upgrade_progress_row,
        upgrade_prompt_action_from_input,
    };
    use crate::application::operations::upgrade_op::{UpdateCheckRow, UpdateCheckStatus};
    use crate::models::common::enums::{Channel, Provider};
    use crate::services::packaging::{PackagePhase, PackageProgressEvent};

    #[test]
    fn upgrade_prompt_accepts_changelog_option() {
        assert!(matches!(
            upgrade_prompt_action_from_input("c").expect("changelog"),
            UpgradePromptAction::Changelog
        ));
        assert!(matches!(
            upgrade_prompt_action_from_input("changelog").expect("changelog"),
            UpgradePromptAction::Changelog
        ));
        assert!(matches!(
            upgrade_prompt_action_from_input("").expect("default yes"),
            UpgradePromptAction::Proceed
        ));
        assert!(upgrade_prompt_action_from_input("n").is_err());
    }

    #[test]
    fn upgrade_progress_row_renders_download_and_phase_states() {
        let download = render_upgrade_progress_row(
            "gitui",
            PackageProgressEvent::Download {
                downloaded: 512,
                total: 1024,
            },
            5,
        );
        assert!(download.starts_with("gitui Downloading [=======>      ]"));
        assert!(download.contains('/'));

        let checksum = render_upgrade_progress_row(
            "gitui",
            PackageProgressEvent::Checksum {
                checked: 512,
                total: 1024,
            },
            5,
        );
        assert!(checksum.starts_with("gitui Checksumming [=======>      ]"));
        assert!(checksum.contains('/'));
        assert!(!checksum.contains("Downloading"));

        let zsync = render_upgrade_progress_row(
            "gitui",
            PackageProgressEvent::Zsync {
                downloaded: 512,
                total: 1024,
            },
            5,
        );
        assert!(zsync.starts_with("gitui Zsync upgrading [=======>      ]"));
        assert!(zsync.contains('/'));

        assert_eq!(
            render_upgrade_progress_row(
                "zsync",
                PackageProgressEvent::Detail("go build -o <artifact> ./cmd/zsync ...".to_string()),
                5,
            ),
            "zsync go build -o <artifact> ./cmd/zsync ..."
        );

        assert_eq!(
            render_upgrade_progress_row(
                "dz6",
                PackageProgressEvent::Phase(PackagePhase::InstallingCompletions),
                5,
            ),
            "dz6   Installing completions..."
        );
    }

    #[test]
    fn check_table_layout_uses_compact_dynamic_columns() {
        let rows = vec![UpdateCheckRow {
            name: "ripgrep".to_string(),
            channel: Some(Channel::Stable),
            provider: Some(Provider::Github),
            status: UpdateCheckStatus::Failed {
                error: "rate limited".to_string(),
            },
        }];

        let layout = CheckTableLayout::all_rows(&rows);

        assert_eq!(layout.name, "ripgrep".len());
        assert_eq!(layout.channel, "Channel".len());
        assert_eq!(layout.source, "Source".len());
    }

    #[test]
    fn check_table_layout_caps_long_names() {
        let rows = vec![UpdateCheckRow {
            name: "a-very-long-package-name".to_string(),
            channel: Some(Channel::Nightly),
            provider: Some(Provider::WebScraper),
            status: UpdateCheckStatus::Failed {
                error: "rate limited".to_string(),
            },
        }];

        let layout = CheckTableLayout::all_rows(&rows);

        assert_eq!(layout.name, 18);
        assert_eq!(layout.channel, "Channel".len());
        assert_eq!(layout.source, 7);
    }

    #[test]
    fn json_check_rows_serializes_nushell_friendly_records() {
        let rows = json_check_rows(vec![
            UpdateCheckRow {
                name: "ripgrep".to_string(),
                channel: Some(Channel::Stable),
                provider: Some(Provider::Github),
                status: UpdateCheckStatus::UpdateAvailable {
                    current: "15.0.0".to_string(),
                    latest: "15.1.0".to_string(),
                },
            },
            UpdateCheckRow {
                name: "missing".to_string(),
                channel: None,
                provider: None,
                status: UpdateCheckStatus::NotInstalled,
            },
        ]);

        let json = serde_json::to_value(rows).expect("serialize check rows");
        assert_eq!(json[0]["state"], "update_available");
        assert_eq!(json[0]["channel"], "Stable");
        assert_eq!(json[0]["provider"], "github");
        assert_eq!(json[0]["current"], "15.0.0");
        assert_eq!(json[0]["latest"], "15.1.0");
        assert_eq!(json[1]["state"], "not_installed");
        assert!(json[1]["current"].is_null());
    }
}
