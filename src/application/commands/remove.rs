use anyhow::Result;
use console::strip_ansi_codes;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    application::operations::remove_operation::RemoveOperation,
    output::{self, SizeImpactRow, Status, TransactionRow},
    services::packaging::{
        PackageProgressEvent,
        disk_impact::{ByteEstimate, DiskImpact, SignedByteEstimate},
    },
    services::storage::{
        metadata_storage::MetadataStorage,
        package_storage::PackageStorage,
        transaction_storage::{
            TransactionKind, TransactionLog, UndoActionKind, package_failed, package_success,
            planned_packages, undo,
        },
    },
    utils::static_paths::UpstreamPaths,
};

fn render_remove_progress(
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

fn render_remove_progress_row(name: &str, event: PackageProgressEvent) -> String {
    let status = match event {
        PackageProgressEvent::Phase(phase) => phase.label().to_string(),
        PackageProgressEvent::Download { .. } => "Downloading package ...".to_string(),
        PackageProgressEvent::Warning(message) => message,
    };
    format!(" {:<28} {}", name, status)
}

fn rollback_size_rows(rollback_impact: SignedByteEstimate) -> Vec<SizeImpactRow> {
    if matches!(rollback_impact.bytes, Some(0)) {
        Vec::new()
    } else {
        vec![SizeImpactRow::new("Rollback storage", rollback_impact)]
    }
}

pub fn run(names: Vec<String>, purge: bool, force: bool, dry_run: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;

    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;

    if names.is_empty() {
        return Err(anyhow::anyhow!("At least one package name is required"));
    }
    let old_versions = names
        .iter()
        .map(|name| {
            (
                name.clone(),
                package_storage
                    .get_package_by_name(name)
                    .map(|package| package.version.to_string()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut package_remover =
        RemoveOperation::new(&mut package_storage, &mut metadata_storage, &paths);

    if dry_run {
        return run_dry_run(names, purge, &mut package_remover);
    }

    let impact_rows = package_remover.transaction_impact_rows(&names, purge)?;
    let impact = impact_rows
        .iter()
        .fold(DiskImpact::empty(), |total, (_, _, impact)| {
            total + impact.clone()
        });
    let transaction_rows = impact_rows
        .iter()
        .map(|(name, version, impact)| {
            TransactionRow::single_version(name, version, impact.net, ByteEstimate::exact(0))
        })
        .collect::<Vec<_>>();
    let rollback_impact = package_remover.estimate_rollback_impact(&names, purge);
    let size_rows = rollback_size_rows(rollback_impact);
    output::print_transaction_table_with_size_rows(
        &transaction_rows,
        &impact,
        "Net disk change:",
        &size_rows,
    );
    output::confirm_or_cancel("Proceed with removal?", true)?;
    let transaction = TransactionLog::start(
        &paths,
        TransactionKind::Remove,
        planned_packages(names.clone()),
        if purge {
            None
        } else {
            undo(UndoActionKind::RestoreRollback, names.clone())
        },
    )?;

    let overall_pb = ProgressBar::new(names.len() as u64);
    overall_pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    overall_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Removed {pos}/{len} packages{msg}",
    )?);
    overall_pb.set_position(0);
    overall_pb.enable_steady_tick(Duration::from_millis(120));

    let overall_pb_ref = overall_pb.clone();
    let mut overall_progress_callback = Some(move |done: u32, total: u32| {
        overall_pb_ref.set_length(total as u64);
        overall_pb_ref.set_position(done as u64);
    });

    let overall_pb_for_messages = overall_pb.clone();
    let active_progress_rows: Arc<Mutex<BTreeMap<String, String>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let completed_progress_rows: Arc<Mutex<BTreeMap<String, String>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let persistent_completion_rows = Arc::new(Mutex::new(Vec::new()));
    let active_rows_for_messages = Arc::clone(&active_progress_rows);
    let completed_rows_for_messages = Arc::clone(&completed_progress_rows);
    let completion_rows_ref = Arc::clone(&persistent_completion_rows);
    let mut message_callback = Some(move |msg: &str| {
        if let Some(key) = completion_message_key(msg) {
            if let Ok(mut rows) = active_rows_for_messages.lock() {
                rows.remove(&key);
            }
            if let Ok(mut rows) = completed_rows_for_messages.lock() {
                rows.insert(key, msg.to_string());
            }
            if let Ok(mut rows) = completion_rows_ref.lock() {
                rows.push(msg.to_string());
            }
            let message = match (
                completed_rows_for_messages.lock(),
                active_rows_for_messages.lock(),
            ) {
                (Ok(completed), Ok(active)) => render_remove_progress(&completed, &active),
                _ => String::new(),
            };
            overall_pb_for_messages.set_message(message);
        }
    });
    let remove_pb_for_progress = overall_pb.clone();
    let active_rows_for_progress = Arc::clone(&active_progress_rows);
    let completed_rows_for_progress = Arc::clone(&completed_progress_rows);
    let mut progress_callback = Some(move |name: &str, event: PackageProgressEvent| {
        if let Ok(mut rows) = active_rows_for_progress.lock() {
            rows.insert(name.to_string(), render_remove_progress_row(name, event));
        }
        let message = match (
            completed_rows_for_progress.lock(),
            active_rows_for_progress.lock(),
        ) {
            (Ok(completed), Ok(active)) => render_remove_progress(&completed, &active),
            _ => String::new(),
        };
        remove_pb_for_progress.set_message(message);
    });

    let bulk_result = package_remover.remove_bulk(
        &names,
        &purge,
        &force,
        &mut message_callback,
        &mut overall_progress_callback,
        &mut progress_callback,
    );
    let (removed, failed) = match bulk_result {
        Ok(result) => result,
        Err(err) => {
            overall_pb.finish_and_clear();
            transaction.fail(
                remove_transaction_packages(&names, &old_versions, 0),
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
        remove_transaction_packages_from_completion_rows(&names, &old_versions, &completion_rows);
    if failed > 0 {
        transaction.fail(
            transaction_packages,
            format!("{failed} package(s) failed to be removed"),
        )?;
        println!(
            "{}",
            output::warning(format!(
                "Removal complete: {} removed, {} failed.",
                removed, failed
            ))
        );
    } else {
        transaction.complete(transaction_packages)?;
        println!(
            "{}",
            output::success(format!("Removal complete: {} removed, 0 failed.", removed))
        );
    }

    Ok(())
}

fn remove_success_package(
    name: &str,
    old_version: Option<String>,
) -> crate::services::storage::transaction_storage::TransactionPackage {
    let mut package = package_success(name.to_string());
    package.old_version = old_version;
    package
}

fn remove_failed_package(
    name: &str,
    old_version: Option<String>,
    error: impl Into<String>,
) -> crate::services::storage::transaction_storage::TransactionPackage {
    let mut package = package_failed(name.to_string(), error);
    package.old_version = old_version;
    package
}

fn remove_transaction_packages(
    names: &[String],
    old_versions: &BTreeMap<String, Option<String>>,
    succeeded_count: usize,
) -> Vec<crate::services::storage::transaction_storage::TransactionPackage> {
    names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let old_version = old_versions.get(name).cloned().flatten();
            if idx < succeeded_count {
                remove_success_package(name, old_version)
            } else {
                remove_failed_package(name, old_version, "removal failed")
            }
        })
        .collect()
}

fn remove_transaction_packages_from_completion_rows(
    names: &[String],
    old_versions: &BTreeMap<String, Option<String>>,
    rows: &[String],
) -> Vec<crate::services::storage::transaction_storage::TransactionPackage> {
    let statuses = rows
        .iter()
        .filter_map(|row| completion_message_result(row))
        .collect::<BTreeMap<_, _>>();
    names
        .iter()
        .map(|name| {
            let old_version = old_versions.get(name).cloned().flatten();
            match statuses.get(name).copied() {
                Some(true) => remove_success_package(name, old_version),
                Some(false) => remove_failed_package(name, old_version, "removal failed"),
                None => remove_failed_package(name, old_version, "removal result unavailable"),
            }
        })
        .collect()
}

fn run_dry_run(
    names: Vec<String>,
    purge: bool,
    package_remover: &mut RemoveOperation<'_>,
) -> Result<()> {
    println!("{}", output::title("Remove preview"));
    output::kv("Purge", if purge { "yes" } else { "no" });
    let (impact, _, estimate_failed) = package_remover.estimate_bulk_impact(&names, purge);
    let rollback_impact = package_remover.estimate_rollback_impact(&names, purge);
    let size_rows = rollback_size_rows(rollback_impact);
    output::print_disk_impact_with_size_rows(&impact, &size_rows, false);
    if estimate_failed > 0 {
        output::action_note(format!(
            "{estimate_failed} package(s) could not be included in disk estimate"
        ));
    }
    output::action_note("resolve only (no remove, no purge, no metadata changes)");
    println!();

    let mut message_callback = Some(|_: &str| {});
    if names.len() > 1 {
        let mut planned = 0_u32;
        let mut failed = 0_u32;
        for name in &names {
            match package_remover.preview_single(name, &purge, &mut message_callback) {
                Ok(_) => {
                    planned += 1;
                    output::status_line(
                        Status::Plan,
                        name,
                        if purge {
                            "remove package files + purge app-owned data"
                        } else {
                            "remove package files"
                        },
                    );
                }
                Err(err) => {
                    failed += 1;
                    output::status_line(Status::Fail, name, output::error_summary(&err));
                }
            }
        }
        println!();
        let status = if failed > 0 { Status::Warn } else { Status::Ok };
        output::status_line(
            status,
            "summary",
            format!("{planned} planned, {failed} failed"),
        );
        return Ok(());
    }

    package_remover.preview_single(&names[0], &purge, &mut message_callback)?;
    output::status_line(
        Status::Plan,
        &names[0],
        if purge {
            "remove package files + purge app-owned data"
        } else {
            "remove package files"
        },
    );
    println!();
    output::status_line(Status::Ok, "summary", "1 planned, 0 failed");
    Ok(())
}
