use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::operations::rollback_op::{
        RollbackListRow, RollbackOperation, RollbackPackageOutcome, RollbackPackageStatus,
        RollbackPreview, RollbackPreviewRow,
    },
    output::{self, Status, TransactionRow},
    storage::rollback::RollbackSource,
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

pub fn run(
    names: Vec<String>,
    list: bool,
    prune: Option<Vec<String>>,
    dry_run: bool,
) -> Result<()> {
    let mut operation = RollbackOperation::new()?;

    match rollback_mode(names, list, prune)? {
        RollbackMode::List => run_list(&mut operation),
        RollbackMode::Restore(names) => run_restore(names, dry_run, &mut operation),
        RollbackMode::Prune(names) => run_prune(names, dry_run, &mut operation),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RollbackMode {
    List,
    Restore(Vec<String>),
    Prune(Vec<String>),
}

fn rollback_mode(
    names: Vec<String>,
    list: bool,
    prune: Option<Vec<String>>,
) -> Result<RollbackMode> {
    if list {
        if !names.is_empty() || prune.is_some() {
            bail!("--list cannot be combined with package names or --prune");
        }
        return Ok(RollbackMode::List);
    }

    if let Some(prune) = prune {
        if !names.is_empty() {
            bail!("--prune cannot be combined with rollback package names");
        }
        if prune.is_empty() {
            return Ok(RollbackMode::Prune(Vec::new()));
        }
        if prune.iter().any(|name| name.eq_ignore_ascii_case("all")) {
            if prune.len() != 1 {
                bail!("--prune all cannot be combined with package names");
            }
            return Ok(RollbackMode::Prune(Vec::new()));
        }
        return Ok(RollbackMode::Prune(prune));
    }

    if names.is_empty() {
        bail!(
            "Package name required. Run 'upstream rollback --list' to see available rollback artifacts."
        );
    }

    Ok(RollbackMode::Restore(names))
}

fn run_restore(names: Vec<String>, dry_run: bool, operation: &mut RollbackOperation) -> Result<()> {
    if names.is_empty() {
        return Ok(());
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

    if outcome.failed > 0 {
        anyhow::bail!("{} package rollback(s) failed", outcome.failed);
    }

    Ok(())
}

fn run_list(operation: &mut RollbackOperation) -> Result<()> {
    let rows = operation.list_rows();
    if rows.is_empty() {
        println!("{}", output::warning("No rollback artifacts found."));
        return Ok(());
    }

    println!("{}", output::title("Rollback artifacts"));
    print_list_rows(&rows);
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

    let pb = ProgressBar::new(preview.target_names.len() as u64);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Pruned {pos}/{len} rollback package(s){msg}",
    )?);
    pb.set_position(0);
    pb.enable_steady_tick(Duration::from_millis(120));

    let prune_pb = pb.clone();
    let mut progress = Some(move |name: &str, current: usize, total: usize| {
        prune_pb.set_length(total as u64);
        prune_pb.set_position(current as u64);
        prune_pb.set_message(format!("\n {:<28} Pruning rollback artifacts ...", name));
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

fn print_list_rows(rows: &[RollbackListRow]) {
    let name_width = rows
        .iter()
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(4)
        .max("Name".len())
        .min(28);
    let version_width = rows
        .iter()
        .map(|row| row.version.chars().count())
        .max()
        .unwrap_or(7)
        .max("Version".len())
        .min(18);
    let source_width = "reinstall".len().max("Source".len());
    let path_width = 72;
    let table_width = name_width + version_width + source_width + path_width + 3;

    println!(
        "{:<name_width$} {:<version_width$} {:<source_width$} Install path",
        "Name", "Version", "Source",
    );
    println!("{}", output::divider(table_width));
    for row in rows {
        println!(
            "{:<name_width$} {:<version_width$} {:<source_width$} {}",
            output::truncate_end(&row.name, name_width),
            output::truncate_end(&row.version, version_width),
            rollback_source_label(&row.source),
            output::truncate_end(&row.install_path, path_width),
        );
    }
}

fn rollback_source_label(source: &RollbackSource) -> &'static str {
    match source {
        RollbackSource::Upgrade => "upgrade",
        RollbackSource::Reinstall => "reinstall",
        RollbackSource::Remove => "remove",
    }
}

#[cfg(test)]
mod tests {
    use super::{RollbackMode, rollback_mode};

    #[test]
    fn rollback_prune_without_names_targets_all_packages() {
        assert_eq!(
            rollback_mode(Vec::new(), false, Some(Vec::new())).expect("mode"),
            RollbackMode::Prune(Vec::new())
        );
    }

    #[test]
    fn rollback_prune_all_remains_supported_as_all_packages() {
        assert_eq!(
            rollback_mode(Vec::new(), false, Some(vec!["all".to_string()])).expect("mode"),
            RollbackMode::Prune(Vec::new())
        );
    }

    #[test]
    fn rollback_prune_rejects_all_combined_with_names() {
        let err = rollback_mode(
            Vec::new(),
            false,
            Some(vec!["all".to_string(), "ripgrep".to_string()]),
        )
        .expect_err("combined all should fail");

        assert!(err.to_string().contains("cannot be combined"));
    }
}
