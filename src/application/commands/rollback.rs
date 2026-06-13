use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::operations::rollback_operation::{
        RollbackOperation, RollbackPackageOutcome, RollbackPackageStatus, RollbackPreview,
        RollbackPreviewRow,
    },
    output::{self, Status, TransactionRow},
};

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

fn transaction_rows(rows: &[RollbackPreviewRow]) -> Vec<TransactionRow> {
    rows.iter().map(TransactionRow::from).collect()
}

fn show_restore_preview(preview: &RollbackPreview) {
    println!("{}", output::title("Rollback preview"));
    if preview.rows.is_empty() {
        println!(
            "{}",
            output::warning("No rollback artifacts found for selected packages.")
        );
    } else {
        output::print_transaction_table(
            &transaction_rows(&preview.rows),
            &preview.impact,
            "Net disk change:",
        );
    }
    show_missing_names(&preview.missing_names);
}

fn show_prune_preview(preview: &RollbackPreview, dry_run: bool) {
    if dry_run {
        println!("{}", output::title("Rollback prune preview"));
    }
    if preview.rows.is_empty() {
        println!("{}", output::warning("No rollback artifacts to prune."));
    } else {
        output::print_transaction_table(
            &transaction_rows(&preview.rows),
            &preview.impact,
            "Net disk change:",
        );
    }
    show_missing_names(&preview.missing_names);
}

fn show_missing_names(names: &[String]) {
    for name in names {
        output::status_line(Status::Fail, name, "no rollback data found");
    }
}

pub fn run(names: Vec<String>, prune: bool, dry_run: bool) -> Result<()> {
    let mut operation = RollbackOperation::new()?;

    if prune {
        return run_prune(names, dry_run, &mut operation);
    }

    let preview = operation.restore_preview(&names)?;

    if dry_run {
        show_restore_preview(&preview.preview);
        for target in &preview.targets {
            output::status_line(
                Status::Plan,
                &target.name,
                format!(
                    "restore rollback from {} ({:?})",
                    target.install_path, target.source
                ),
            );
        }
        output::action_note("resolve only (no restore, no prune, no metadata changes)");
        return Ok(());
    }

    show_restore_preview(&preview.preview);
    let restorable_names = operation.restorable_names(&names);
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

    let phase_pb = pb.clone();
    let mut progress = Some(move |package_name: &str, line: &str| {
        phase_pb.set_message(format!(
            "Restoring rollback for {package_name}\n {:<28} {}",
            package_name,
            restore_phase_label(line)
        ));
    });
    let outcome = operation.restore(&restorable_names, &mut progress)?;
    pb.finish_and_clear();

    print_completion_lines(&outcome.packages);

    if outcome.failed > 0 {
        println!(
            "{}",
            output::warning(format!(
                "Rollback complete: {} restored, {} failed.",
                outcome.restored, outcome.failed
            ))
        );
    } else {
        println!(
            "{}",
            output::success(format!(
                "Rollback complete: {} restored, 0 failed.",
                outcome.restored
            ))
        );
    }

    Ok(())
}

fn run_prune(names: Vec<String>, dry_run: bool, operation: &mut RollbackOperation) -> Result<()> {
    let preview = operation.prune_preview(names);

    if dry_run {
        show_prune_preview(&preview.preview, true);
        output::action_note("resolve only (no prune, no metadata changes)");
        return Ok(());
    }

    if !preview.target_names.is_empty() {
        show_prune_preview(&preview.preview, false);
        output::confirm_or_cancel(
            format!(
                "Prune rollback artifacts for {} package(s)?",
                preview.target_names.len()
            ),
            false,
        )?;
    }

    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Pruning rollback artifacts");

    let prune_pb = pb.clone();
    let mut progress = Some(move |name: &str, current: usize, total: usize| {
        prune_pb.set_message(format!(
            "Pruning rollback artifacts for {:<28} ({}/{})",
            name, current, total
        ));
    });
    let outcome = operation.prune(&preview.target_names, &mut progress);
    pb.finish_and_clear();
    let outcome = outcome?;

    if preview.target_names.is_empty() {
        println!("{}", output::warning("No rollback artifacts to prune."));
    } else {
        println!(
            "{}",
            output::success(format!(
                "Rollback prune complete: {} pruned, {} missing.",
                outcome.pruned, outcome.missing
            ))
        );
    }

    Ok(())
}

fn print_completion_lines(packages: &[RollbackPackageOutcome]) {
    let width = output::status_subject_width(packages.iter().map(|package| package.name.as_str()));
    for package in packages {
        match &package.status {
            RollbackPackageStatus::Succeeded => {
                println!(
                    "{}",
                    output::status_line_text_with_width(
                        Status::Ok,
                        &package.name,
                        "restored",
                        width
                    )
                );
            }
            RollbackPackageStatus::Failed { error } => {
                println!(
                    "{}",
                    output::status_line_text_with_width(Status::Fail, &package.name, error, width)
                );
            }
            RollbackPackageStatus::Skipped { reason } => {
                println!(
                    "{}",
                    output::status_line_text_with_width(Status::Warn, &package.name, reason, width)
                );
            }
        }
    }
}
