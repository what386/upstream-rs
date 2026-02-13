use crate::{
    application::operations::upgrade_operation::{
        UpdateCheckRow, UpdateCheckStatus, UpgradeOperation,
    },
    providers::provider_manager::ProviderManager,
    services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::time::Duration;

fn print_upgrade_titlebar() {
    println!(
        "    {:<28} {:<10} {:<3} {:<10} {}",
        "Name", "Channel", "Op", "Remote", "Download"
    );
}

pub async fn run(names: Option<Vec<String>>, force_option: bool, check_option: bool) -> Result<()> {
    let paths = UpstreamPaths::new();
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let github_token = config.get_config().github.api_token.as_deref();
    let gitlab_token = config.get_config().gitlab.api_token.as_deref();

    // Get GitLab base_url from installed packages
    let packages = package_storage.get_all_packages();
    let gitlab_base_url = packages.iter().find_map(|p| p.base_url.as_deref());
    let installed_package_count = packages.len();

    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitlab_base_url)?;
    let mut package_upgrade =
        UpgradeOperation::new(&provider_manager, &mut package_storage, &paths)?;

    // Handle --check flag
    if check_option {
        return run_check(package_upgrade, names).await;
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
            style(format!("Upgrading {} package(s)", installed_package_count)).cyan()
        );
        print_upgrade_titlebar();
        package_upgrade
            .upgrade_all(
                &force_option,
                &mut download_progress_callback,
                &mut overall_progress_callback,
                &mut message_callback,
            )
            .await?;

        download_pb.finish_and_clear();
        overall_pb.finish_with_message("Upgrade complete");
        return Ok(());
    }

    let name_vec = names.unwrap();
    println!(
        "{}",
        style(format!("Upgrading {} package(s)", name_vec.len())).cyan()
    );
    if name_vec.len() > 1 {
        print_upgrade_titlebar();
        package_upgrade
            .upgrade_bulk(
                &name_vec,
                &force_option,
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
                &mut download_progress_callback,
                &mut message_callback,
            )
            .await?;
    }

    download_pb.finish_and_clear();
    overall_pb.finish_with_message("Upgrade complete");
    println!("{}", style("Upgrade complete.").green());

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

    println!("Checking for updates...\n");

    if !display_rows.is_empty() {
        println!(
            "{:<5} {:<28} {:<10} {:<3} {:<10} {}",
            "State", "Name", "Branch", "Op", "Remote", "Version"
        );
    }

    for row in display_rows {
        let (status, op, version) = match &row.status {
            UpdateCheckStatus::UpdateAvailable { current, latest } => (
                "[✓]".to_string(),
                "u".to_string(),
                format!("{current} -> {latest}"),
            ),
            UpdateCheckStatus::Failed { error } => {
                ("[!]".to_string(), "!".to_string(), truncate_cell(error, 32))
            }
            UpdateCheckStatus::NotInstalled => (
                "[x]".to_string(),
                "?".to_string(),
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
            "{:<5} {:<28} {:<10} {:<3} {:<10} {}",
            status,
            truncate_cell(&row.name, 28),
            truncate_cell(&branch, 10),
            op,
            truncate_cell(&remote, 10),
            version
        );
    }

    println!();
    println!(
        "Checks complete. {} available, {} up to date, {} failed, {} not installed.",
        available, up_to_date, failed, not_installed
    );
}

async fn run_check(
    package_upgrade: UpgradeOperation<'_>,
    names: Option<Vec<String>>,
) -> Result<()> {
    let rows = match names {
        None => package_upgrade.check_all_detailed().await,
        Some(name_vec) => package_upgrade.check_selected_detailed(&name_vec).await,
    };

    render_check_table(&rows);

    Ok(())
}
