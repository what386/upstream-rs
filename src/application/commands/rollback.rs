use anyhow::{Result, anyhow};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::output::{self, Status, TransactionRow};
use crate::services::packaging::RollbackManager;
use crate::services::packaging::disk_impact::{ByteEstimate, DiskImpact, SignedByteEstimate};
use crate::services::storage::{
    metadata_storage::MetadataStorage, package_storage::PackageStorage,
    rollback_storage::RollbackStorage,
};
use crate::utils::static_paths::UpstreamPaths;

fn restore_preview_rows(names: &[String], manager: &RollbackManager<'_>) -> Vec<TransactionRow> {
    names
        .iter()
        .filter_map(|name| {
            let record = manager.rollback_record(name)?;
            let pkg = &record.package_snapshot;
            Some(TransactionRow::single_version(
                format!("{}/{}", pkg.provider, pkg.name),
                pkg.version.to_string(),
                manager
                    .estimate_restore_impact(name)
                    .map(|impact| impact.net)
                    .unwrap_or(SignedByteEstimate::exact(0)),
                ByteEstimate::exact(0),
            ))
        })
        .collect()
}

fn prune_preview_rows(names: &[String], manager: &RollbackManager<'_>) -> Vec<TransactionRow> {
    names
        .iter()
        .filter_map(|name| {
            let record = manager.rollback_record(name)?;
            let pkg = &record.package_snapshot;
            Some(TransactionRow::single_version(
                format!("{}/{}", pkg.provider, pkg.name),
                pkg.version.to_string(),
                manager
                    .estimate_prune_impact(name)
                    .map(|impact| impact.net)
                    .unwrap_or(SignedByteEstimate::exact(0)),
                ByteEstimate::exact(0),
            ))
        })
        .collect()
}

fn restore_phase_label(message: &str) -> &'static str {
    if message.starts_with("Removing current installation for ") {
        "Removing current install ..."
    } else if message.starts_with("Restoring rollback artifact for ") {
        "Restoring rollback artifact ..."
    } else if message.starts_with("Restoring '") && message.contains("' to PATH") {
        "Restoring PATH entries ..."
    } else if message.starts_with("Restoring symlink for ") {
        "Restoring runtime links ..."
    } else if message.starts_with("Installing completion scripts for ") {
        "Installing completions ..."
    } else {
        "Restoring rollback ..."
    }
}

fn show_restore_preview(rows: &[TransactionRow], impact: &DiskImpact, names: &[String]) {
    println!("{}", output::title("Rollback preview"));
    if rows.is_empty() {
        println!(
            "{}",
            output::warning("No rollback artifacts found for selected packages.")
        );
    } else {
        output::print_transaction_table(rows, impact, "Net disk change:");
    }
    for name in names {
        if !rows
            .iter()
            .any(|row| row.package.ends_with(&format!("/{name}")))
        {
            output::status_line(Status::Fail, name, "no rollback data found");
        }
    }
}

fn show_prune_preview(
    rows: &[TransactionRow],
    impact: &DiskImpact,
    names: &[String],
    dry_run: bool,
) {
    if dry_run {
        println!("{}", output::title("Rollback prune preview"));
    }
    if rows.is_empty() {
        println!("{}", output::warning("No rollback artifacts to prune."));
    } else {
        output::print_transaction_table(rows, impact, "Net disk change:");
    }
    for name in names {
        if !rows
            .iter()
            .any(|row| row.package.ends_with(&format!("/{name}")))
        {
            output::status_line(Status::Fail, name, "no rollback data found");
        }
    }
}

pub fn run(names: Vec<String>, prune: bool, dry_run: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let rollback_file = RollbackManager::rollback_file_path(&paths);
    let mut rollback_storage = RollbackStorage::new(&rollback_file)?;

    let mut manager = RollbackManager::new(
        &paths,
        &mut package_storage,
        &mut metadata_storage,
        &mut rollback_storage,
    );

    if prune {
        return run_prune(names, dry_run, &mut manager);
    }

    if names.is_empty() {
        return Err(anyhow!(
            "At least one package name is required unless --prune is provided"
        ));
    }

    let preview_rows = restore_preview_rows(&names, &manager);
    let impact = estimate_restore_impact(&names, &manager);

    if dry_run {
        show_restore_preview(&preview_rows, &impact, &names);
        for name in &names {
            let Some(record) = manager.rollback_record(name) else {
                continue;
            };

            let target = record
                .package_snapshot
                .install_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<missing>".to_string());
            output::status_line(
                Status::Plan,
                name,
                format!("restore rollback from {} ({:?})", target, record.source),
            );
        }
        output::action_note("resolve only (no restore, no prune, no metadata changes)");
        return Ok(());
    }

    show_restore_preview(&preview_rows, &impact, &names);
    let restorable_names = names
        .iter()
        .filter(|name| manager.rollback_record(name).is_some())
        .cloned()
        .collect::<Vec<_>>();
    if restorable_names.is_empty() {
        println!(
            "{}",
            output::warning("No rollback artifacts to restore for selected packages.")
        );
        return Ok(());
    }
    output::confirm_or_cancel(
        format!(
            "Restore rollback for {} package(s)?",
            restorable_names.len()
        ),
        false,
    )?;

    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Restoring rollback");

    let mut restored = 0_u32;
    let mut failed = 0_u32;
    let mut completion_lines = Vec::new();
    for name in &restorable_names {
        let package_name = name.clone();
        let phase_pb = pb.clone();
        let mut msg = Some(move |line: &str| {
            phase_pb.set_message(format!(
                "Restoring rollback for {package_name}\n {:<28} {}",
                package_name,
                restore_phase_label(line)
            ));
        });
        match manager.restore_package(name, &mut msg) {
            Ok(_) => {
                completion_lines.push(output::status_line_text(Status::Ok, name, "restored"));
                restored += 1;
            }
            Err(err) => {
                completion_lines.push(output::status_line_text(
                    Status::Fail,
                    name,
                    output::error_summary(&err),
                ));
                failed += 1;
            }
        }
    }
    pb.finish_and_clear();
    for line in completion_lines {
        println!("{line}");
    }

    if failed > 0 {
        println!(
            "{}",
            output::warning(format!(
                "Rollback complete: {} restored, {} failed.",
                restored, failed
            ))
        );
    } else {
        println!(
            "{}",
            output::success(format!(
                "Rollback complete: {} restored, 0 failed.",
                restored
            ))
        );
    }

    Ok(())
}

fn run_prune(names: Vec<String>, dry_run: bool, manager: &mut RollbackManager<'_>) -> Result<()> {
    let target_names = if names.is_empty() {
        manager.rollback_packages()
    } else {
        names
    };
    let preview_rows = prune_preview_rows(&target_names, manager);
    let impact = estimate_prune_impact(&target_names, manager);

    if dry_run {
        show_prune_preview(&preview_rows, &impact, &target_names, true);
        output::action_note("resolve only (no prune, no metadata changes)");
        return Ok(());
    }

    if !target_names.is_empty() {
        show_prune_preview(&preview_rows, &impact, &target_names, false);
        output::confirm_or_cancel(
            format!(
                "Prune rollback artifacts for {} package(s)?",
                target_names.len()
            ),
            false,
        )?;
    }

    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Pruning rollback artifacts");

    let mut pruned = 0_u32;
    let mut missing = 0_u32;
    let total = target_names.len();
    for (idx, name) in target_names.iter().enumerate() {
        pb.set_message(format!(
            "Pruning rollback artifacts for {:<28} ({}/{})",
            name,
            idx + 1,
            total
        ));
        if manager.prune_package(name)? {
            pruned += 1;
        } else {
            missing += 1;
        }
    }
    pb.finish_and_clear();

    if target_names.is_empty() {
        println!("{}", output::warning("No rollback artifacts to prune."));
    } else {
        println!(
            "{}",
            output::success(format!(
                "Rollback prune complete: {} pruned, {} missing.",
                pruned, missing
            ))
        );
    }

    Ok(())
}

fn estimate_restore_impact(names: &[String], manager: &RollbackManager<'_>) -> DiskImpact {
    names
        .iter()
        .filter_map(|name| manager.estimate_restore_impact(name))
        .fold(DiskImpact::empty(), |total, impact| total + impact)
}

fn estimate_prune_impact(names: &[String], manager: &RollbackManager<'_>) -> DiskImpact {
    names
        .iter()
        .filter_map(|name| manager.estimate_prune_impact(name))
        .fold(DiskImpact::empty(), |total, impact| total + impact)
}
