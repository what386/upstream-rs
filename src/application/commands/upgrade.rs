use crate::{
    application::commands::changelog::changelog_text_for_package,
    application::operations::upgrade_operation::{
        UpdateCheckRow, UpdateCheckStatus, UpgradeOperation, UpgradePreviewEvent,
    },
    models::common::enums::TrustMode,
    output::{self, SizeImpactRow, Status, TransactionRow, TransactionTableLayout},
    providers::provider_manager::ProviderManager,
    services::storage::{
        config_storage::ConfigStorage,
        package_storage::PackageStorage,
        trust_storage::TrustStorage,
        transaction_storage::{
            TransactionKind, TransactionLog, UndoActionKind, package_failed, package_success,
            planned_packages, undo,
        },
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use console::strip_ansi_codes;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

fn upgrade_transaction_row(
    row: &crate::application::operations::upgrade_operation::UpgradePreviewRow,
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
    completed_rows: &BTreeMap<String, String>,
    active_rows: &BTreeMap<String, String>,
) -> String {
    if completed_rows.is_empty() && active_rows.is_empty() {
        return String::new();
    }

    let rows = completed_rows
        .values()
        .chain(active_rows.values())
        .cloned()
        .collect::<Vec<_>>();
    format!("\n{}", rows.join("\n"))
}

fn completion_message_key(message: &str) -> Option<String> {
    let cleaned = strip_ansi_codes(message);
    let rest = cleaned
        .trim_start()
        .strip_prefix("[ok]")
        .or_else(|| cleaned.trim_start().strip_prefix("[fail]"))?
        .trim_start();
    rest.split_whitespace().next().map(str::to_string)
}

fn completion_message_result(message: &str) -> Option<(String, bool)> {
    let cleaned = strip_ansi_codes(message);
    let trimmed = cleaned.trim_start();
    let (rest, succeeded) = if let Some(rest) = trimmed.strip_prefix("[ok]") {
        (rest, true)
    } else if let Some(rest) = trimmed.strip_prefix("[fail]") {
        (rest, false)
    } else {
        return None;
    };
    let name = rest.split_whitespace().next()?.to_string();
    Some((name, succeeded))
}

pub async fn run(
    names: Option<Vec<String>>,
    force_option: bool,
    check_option: bool,
    machine_readable: bool,
    json: bool,
    trust_mode: TrustMode,
    dry_run: bool,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let trust_storage = TrustStorage::new(&paths.config.trust_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let app_config = config.get_config();
    let github_token = app_config.github.api_token.as_deref();
    let gitlab_token = app_config.gitlab.api_token.as_deref();
    let gitea_token = app_config.gitea.api_token.as_deref();

    let trusted_keys = trust_storage.trusted_signature_keys();

    let provider_manager = ProviderManager::new_with_download_config(
        github_token,
        gitlab_token,
        gitea_token,
        app_config.download,
    )?;
    let mut package_upgrade = UpgradeOperation::new(
        &provider_manager,
        &mut package_storage,
        &paths,
        trusted_keys,
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
        .preview_upgrade_with_events(names.as_deref(), force_option, &mut |event| match event {
            UpgradePreviewEvent::Started { package_width } => {
                let layout = TransactionTableLayout::upgrade_preview(package_width);
                live_layout = Some(layout);

                let pb = ProgressBar::new_spinner();
                pb.set_draw_target(ProgressDrawTarget::stdout_with_hz(10));
                pb.set_style(
                    ProgressStyle::with_template("{spinner:.cyan} {msg}")
                        .expect("valid progress template"),
                );
                pb.enable_steady_tick(Duration::from_millis(120));
                pb.set_message("checking for updates");
                check_pb = Some(pb);
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
    confirm_or_show_changelog(&provider_manager, &preview_rows).await?;
    let tx_names = preview_rows
        .iter()
        .map(|row| row.name.clone())
        .collect::<Vec<_>>();
    let transaction = TransactionLog::start(
        &paths,
        TransactionKind::Upgrade,
        planned_packages(tx_names.clone()),
        undo(UndoActionKind::RestoreRollback, tx_names.clone()),
    )?;

    let overall_pb = ProgressBar::new(preview_rows.len() as u64);
    overall_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Upgraded {pos}/{len} packages{msg}",
    )?);
    overall_pb.enable_steady_tick(Duration::from_millis(120));

    let overall_pb_ref = overall_pb.clone();
    let mut overall_progress_callback = Some(move |done: u32, total: u32| {
        overall_pb_ref.set_length(total as u64);
        overall_pb_ref.set_position(done as u64);
    });

    let message_pb = overall_pb.clone();
    let mut active_progress_rows = BTreeMap::new();
    let mut completed_progress_rows = BTreeMap::new();
    let persistent_completion_rows = Arc::new(Mutex::new(Vec::new()));
    let completion_rows_ref = Arc::clone(&persistent_completion_rows);
    let mut message_callback = Some(move |msg: &str| {
        if let Some(rest) = msg.strip_prefix("__UPGRADE_PROGRESS_ROW__ ") {
            if let Some((name, row)) = rest.split_once('\t') {
                active_progress_rows.insert(name.to_string(), row.to_string());
                message_pb.set_message(render_upgrade_progress(
                    &completed_progress_rows,
                    &active_progress_rows,
                ));
            }
            return;
        }
        if let Some(name) = msg.strip_prefix("__UPGRADE_PROGRESS_DONE__ ") {
            active_progress_rows.remove(name);
            message_pb.set_message(render_upgrade_progress(
                &completed_progress_rows,
                &active_progress_rows,
            ));
            return;
        }
        if msg == "__UPGRADE_PROGRESS_CLEAR__" {
            active_progress_rows.clear();
            message_pb.set_message(render_upgrade_progress(
                &completed_progress_rows,
                &active_progress_rows,
            ));
            return;
        }
        if let Some(key) = completion_message_key(msg) {
            active_progress_rows.remove(&key);
            completed_progress_rows.insert(key, msg.to_string());
            if let Ok(mut rows) = completion_rows_ref.lock() {
                rows.push(msg.to_string());
            }
            message_pb.set_message(render_upgrade_progress(
                &completed_progress_rows,
                &active_progress_rows,
            ));
            return;
        }
        message_pb.println(msg);
    });
    let mut no_download_progress: Option<fn(u64, u64)> = None;
    let bulk_result = package_upgrade
        .upgrade_resolved_bulk(
            &preview_rows,
            trust_mode,
            &mut no_download_progress,
            &mut overall_progress_callback,
            &mut message_callback,
        )
        .await;
    let (upgraded, failed) = match bulk_result {
        Ok(result) => result,
        Err(err) => {
            overall_pb.finish_and_clear();
            transaction.fail(
                upgrade_transaction_packages(&preview_rows, 0),
                err.to_string(),
            )?;
            return Err(err);
        }
    };

    overall_pb.finish_and_clear();
    let completion_rows = persistent_completion_rows
        .lock()
        .map(|rows| rows.clone())
        .unwrap_or_default();
    for row in &completion_rows {
        println!("{row}");
    }
    let transaction_packages =
        upgrade_transaction_packages_from_completion_rows(&preview_rows, &completion_rows);
    if failed > 0 {
        transaction.fail(
            transaction_packages,
            format!("{failed} package(s) failed to upgrade"),
        )?;
        println!(
            "{}",
            output::warning(format!(
                "Upgrade complete: {} upgraded, {} failed.",
                upgraded, failed
            ))
        );
    } else {
        transaction.complete(transaction_packages)?;
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
    preview_rows: &[crate::application::operations::upgrade_operation::UpgradePreviewRow],
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
    preview_rows: &[crate::application::operations::upgrade_operation::UpgradePreviewRow],
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

fn upgrade_transaction_packages(
    rows: &[crate::application::operations::upgrade_operation::UpgradePreviewRow],
    succeeded_count: usize,
) -> Vec<crate::services::storage::transaction_storage::TransactionPackage> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let mut package = if idx < succeeded_count {
                package_success(row.name.clone())
            } else {
                package_failed(row.name.clone(), "upgrade failed")
            };
            package.old_version = Some(row.old_version.clone());
            package.new_version = Some(row.new_version.clone());
            package
        })
        .collect()
}

fn upgrade_transaction_packages_from_completion_rows(
    rows: &[crate::application::operations::upgrade_operation::UpgradePreviewRow],
    completion_rows: &[String],
) -> Vec<crate::services::storage::transaction_storage::TransactionPackage> {
    let statuses = completion_rows
        .iter()
        .filter_map(|row| completion_message_result(row))
        .collect::<BTreeMap<_, _>>();
    rows.iter()
        .map(|row| {
            let mut package = match statuses.get(&row.name).copied() {
                Some(true) => package_success(row.name.clone()),
                Some(false) => package_failed(row.name.clone(), "upgrade failed"),
                None => package_failed(row.name.clone(), "upgrade result unavailable"),
            };
            package.old_version = Some(row.old_version.clone());
            package.new_version = Some(row.new_version.clone());
            package
        })
        .collect()
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
        println!();
        println!(
            "{}",
            output::section(format!(
                "{:<8} {:<28} {:<10} {:<10} Version",
                "State", "Name", "Channel", "Source"
            ))
        );
    }

    for row in &display_rows {
        let (status, version) = match &row.status {
            UpdateCheckStatus::UpdateAvailable { current, latest } => (
                output::status_cell(Status::Plan).to_string(),
                format!("{current} -> {latest}"),
            ),
            UpdateCheckStatus::Failed { error } => (
                output::status_cell(Status::Fail).to_string(),
                truncate_cell(error, 32),
            ),
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
            "{} {:<28} {:<10} {:<10} {}",
            status,
            truncate_cell(&row.name, 28),
            truncate_cell(&branch, 10),
            truncate_cell(&remote, 10),
            version
        );
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

async fn run_check(
    package_upgrade: UpgradeOperation<'_>,
    names: Option<Vec<String>>,
    machine_readable: bool,
    json: bool,
) -> Result<()> {
    if json {
        let rows = match names {
            None => package_upgrade.check_all_detailed().await,
            Some(name_vec) => package_upgrade.check_selected_detailed(&name_vec).await,
        };
        println!("{}", serde_json::to_string_pretty(&json_check_rows(rows))?);
    } else if machine_readable {
        let updates = match names {
            None => package_upgrade.check_all_machine_readable().await,
            Some(name_vec) => {
                package_upgrade
                    .check_selected_machine_readable(&name_vec)
                    .await
            }
        };
        for (name, oldver, newver) in updates {
            println!("{name} {oldver} {newver}");
        }
    } else {
        let rows = match names {
            None => package_upgrade.check_all_detailed().await,
            Some(name_vec) => package_upgrade.check_selected_detailed(&name_vec).await,
        };
        render_check_table(&rows);
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
        .preview_upgrade(names.as_deref(), force_option)
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
    let rows = match names {
        None => package_upgrade.check_all_detailed().await,
        Some(name_vec) => package_upgrade.check_selected_detailed(&name_vec).await,
    };

    if rows.is_empty() {
        println!("{}", output::warning("No installed packages to check."));
        return Ok(());
    }

    println!(
        "{}",
        output::section(format!(
            "{:<8} {:<28} {:<10} {:<10} Plan",
            "State", "Name", "Channel", "Source"
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
                    "{} {:<28} {:<10} {:<10} {}",
                    output::status_cell(Status::Plan),
                    truncate_cell(&row.name, 28),
                    truncate_cell(&branch, 10),
                    truncate_cell(&remote, 10),
                    plan
                );
            }
            UpdateCheckStatus::UpToDate { current } => {
                if force_option {
                    would_upgrade += 1;
                    println!(
                        "{} {:<28} {:<10} {:<10} force-upgrade {}",
                        output::status_cell(Status::Plan),
                        truncate_cell(&row.name, 28),
                        truncate_cell(&branch, 10),
                        truncate_cell(&remote, 10),
                        current
                    );
                } else {
                    up_to_date += 1;
                    let _ = current;
                }
            }
            UpdateCheckStatus::Failed { error } => {
                failed += 1;
                println!(
                    "{} {:<28} {:<10} {:<10} failed to resolve: {}",
                    output::status_cell(Status::Fail),
                    truncate_cell(&row.name, 28),
                    truncate_cell(&branch, 10),
                    truncate_cell(&remote, 10),
                    truncate_cell(&error, 48)
                );
            }
            UpdateCheckStatus::NotInstalled => {
                not_installed += 1;
                println!(
                    "{} {:<28} {:<10} {:<10} not installed",
                    output::status_cell(Status::Fail),
                    truncate_cell(&row.name, 28),
                    truncate_cell(&branch, 10),
                    truncate_cell(&remote, 10)
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        UpgradePromptAction, completion_message_key, json_check_rows, render_upgrade_progress,
        upgrade_prompt_action_from_input,
    };
    use crate::application::operations::upgrade_operation::{UpdateCheckRow, UpdateCheckStatus};
    use crate::models::common::enums::{Channel, Provider};
    use std::collections::BTreeMap;

    #[test]
    fn upgrade_progress_renders_completed_rows_before_active_rows() {
        let mut completed = BTreeMap::new();
        completed.insert(
            "tally".to_string(),
            "[ok] tally upgraded to 0.13.0 494 B / 494 B".to_string(),
        );

        let mut active = BTreeMap::new();
        active.insert(
            "forge".to_string(),
            "stable/forge u github 1.00 MiB/5.00 MiB".to_string(),
        );
        active.insert(
            "ripgrep".to_string(),
            "stable/ripgrep u github 2.00 MiB/4.00 MiB".to_string(),
        );

        assert_eq!(
            render_upgrade_progress(&completed, &active),
            "\n[ok] tally upgraded to 0.13.0 494 B / 494 B\nstable/forge u github 1.00 MiB/5.00 MiB\nstable/ripgrep u github 2.00 MiB/4.00 MiB"
        );

        active.remove("forge");
        assert_eq!(
            render_upgrade_progress(&completed, &active),
            "\n[ok] tally upgraded to 0.13.0 494 B / 494 B\nstable/ripgrep u github 2.00 MiB/4.00 MiB"
        );

        completed.clear();
        active.clear();
        assert_eq!(render_upgrade_progress(&completed, &active), "");
    }

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
    fn completion_message_key_extracts_package_name() {
        assert_eq!(
            completion_message_key("[ok] tally upgraded to 0.13.0"),
            Some("tally".to_string())
        );
        assert_eq!(
            completion_message_key("[fail] forge failed to upgrade"),
            Some("forge".to_string())
        );
        assert_eq!(completion_message_key("Downloading forge ..."), None);
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
