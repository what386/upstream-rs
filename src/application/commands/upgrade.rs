use crate::{
    application::operations::upgrade_operation::{
        UpdateCheckRow, UpdateCheckStatus, UpgradeOperation,
    },
    application::output::{self, Status},
    models::common::enums::TrustMode,
    providers::provider_manager::ProviderManager,
    services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::time::Duration;

fn print_upgrade_titlebar() {
    println!(
        "    {:<28} {:<10} {:<3} {:<10} Download",
        "Name", "Channel", "Op", "Remote"
    );
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

    let installed_package_count = package_storage.get_all_packages().len();
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

    // Normal upgrade flow
    let mp = MultiProgress::new();
    let download_pb = mp.add(ProgressBar::new(0));
    download_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )?);

    // Keep overall summary at the bottom; task rows are inserted before it.
    let overall_pb = mp.add(ProgressBar::new(0));
    overall_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Upgraded {pos}/{len} packages",
    )?);

    let overall_pb_ref = overall_pb.clone();
    let mut overall_progress_callback = Some(move |done: u32, total: u32| {
        overall_pb_ref.set_length(total as u64);
        overall_pb_ref.set_position(done as u64);
    });

    let download_pb_ref = &download_pb;
    let mut download_progress_callback = Some(move |downloaded: u64, total: u64| {
        download_pb_ref.set_length(total);
        download_pb_ref.set_position(downloaded);
    });

    let message_pb = &overall_pb;
    let mut progress_rows: HashMap<String, ProgressBar> = HashMap::new();
    let mp_ref = &mp;
    let overall_for_rows = overall_pb.clone();
    let mut message_callback = Some(move |msg: &str| {
        if let Some(payload) = msg.strip_prefix("__UPGRADE_PROGRESS_ROW__ ") {
            if let Some((name, row)) = payload.split_once('\t') {
                let pb = progress_rows.entry(name.to_string()).or_insert_with(|| {
                    let pb = mp_ref.insert_before(&overall_for_rows, ProgressBar::new_spinner());
                    pb.set_style(
                        ProgressStyle::with_template("{spinner:.cyan}{msg}")
                            .expect("valid progress template"),
                    );
                    pb.enable_steady_tick(Duration::from_millis(120));
                    pb
                });
                pb.set_message(row.to_string());
            }
            return;
        }
        if let Some(name) = msg.strip_prefix("__UPGRADE_PROGRESS_DONE__ ") {
            if let Some(pb) = progress_rows.remove(name) {
                pb.finish_and_clear();
            }
            return;
        }
        if msg == "__UPGRADE_PROGRESS_CLEAR__" {
            for (_, pb) in progress_rows.drain() {
                pb.finish_and_clear();
            }
            return;
        }
        message_pb.println(msg);
    });

    if names.is_none() {
        println!(
            "{}",
            output::title(format!("Upgrading {} package(s)", installed_package_count))
        );
        print_upgrade_titlebar();
        package_upgrade
            .upgrade_all(
                &force_option,
                trust_mode,
                &mut download_progress_callback,
                &mut overall_progress_callback,
                &mut message_callback,
            )
            .await?;

        download_pb.finish_and_clear();
        overall_pb.finish_with_message("Upgrade complete");
        return Ok(());
    }

    let Some(name_vec) = names else {
        return Ok(());
    };
    println!(
        "{}",
        output::title(format!("Upgrading {} package(s)", name_vec.len()))
    );
    if name_vec.len() > 1 {
        print_upgrade_titlebar();
        package_upgrade
            .upgrade_bulk(
                &name_vec,
                &force_option,
                trust_mode,
                &mut download_progress_callback,
                &mut overall_progress_callback,
                &mut message_callback,
            )
            .await?;
    } else {
        package_upgrade
            .upgrade_single(
                &name_vec[0],
                &force_option,
                trust_mode,
                &mut download_progress_callback,
                &mut message_callback,
            )
            .await?;
    }

    download_pb.finish_and_clear();
    overall_pb.finish_with_message("Upgrade complete");
    println!("{}", output::success("Upgrade complete."));

    Ok(())
}

fn truncate_cell(value: &str, max: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max {
        return value.to_string();
    }

    let mut out = String::new();
    for ch in value.chars().take(max.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    out
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
