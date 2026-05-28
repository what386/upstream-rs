use crate::{
    application::operations::upgrade_operation::{
        UpdateCheckRow, UpdateCheckStatus, UpgradeOperation, UpgradePreviewEvent,
    },
    application::output::{self, Status, TransactionRow, TransactionTableLayout},
    models::common::enums::TrustMode,
    providers::provider_manager::ProviderManager,
    services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::{
    collections::BTreeMap,
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
    let rest = message
        .strip_prefix("[ok] ")
        .or_else(|| message.strip_prefix("[fail] "))?;
    rest.split_whitespace().next().map(str::to_string)
}

pub async fn run(
    names: Option<Vec<String>>,
    force_option: bool,
    check_option: bool,
    machine_readable: bool,
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

    let trusted_keys = app_config.trusted_signature_keys();

    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;
    let mut package_upgrade = UpgradeOperation::new(
        &provider_manager,
        &mut package_storage,
        &paths,
        trusted_keys,
    )?;

    // Handle --check flag
    if check_option {
        return run_check(package_upgrade, names, machine_readable).await;
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
        layout.print_totals(&impact, "Net disk change:");
    } else {
        let transaction_rows = preview_rows
            .iter()
            .map(upgrade_transaction_row)
            .collect::<Vec<_>>();
        output::print_transaction_table(&transaction_rows, &impact, "Net disk change:");
    }
    output::confirm_yes_default_or_cancel("Proceed with installation?")?;

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
    let (upgraded, failed) = package_upgrade
        .upgrade_resolved_bulk(
            &preview_rows,
            trust_mode,
            &mut no_download_progress,
            &mut overall_progress_callback,
            &mut message_callback,
        )
        .await?;

    overall_pb.finish_and_clear();
    if let Ok(rows) = persistent_completion_rows.lock() {
        for row in rows.iter() {
            println!("{row}");
        }
    }
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

async fn run_check(
    package_upgrade: UpgradeOperation<'_>,
    names: Option<Vec<String>>,
    machine_readable: bool,
) -> Result<()> {
    if machine_readable {
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
    let impact = package_upgrade
        .estimate_upgrade_impact(names.as_deref(), force_option)
        .await;
    output::print_disk_impact(&impact);
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
    use super::{completion_message_key, render_upgrade_progress};
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
}
