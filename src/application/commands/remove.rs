use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::operations::remove_operation::RemoveOperation,
    application::output::{self, Status},
    services::storage::{metadata_storage::MetadataStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};

pub fn run(names: Vec<String>, purge: bool, dry_run: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;

    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;

    let mut package_remover =
        RemoveOperation::new(&mut package_storage, &mut metadata_storage, &paths);

    if names.is_empty() {
        return Err(anyhow::anyhow!("At least one package name is required"));
    }

    if dry_run {
        return run_dry_run(names, purge, &mut package_remover);
    }

    let overall_pb = ProgressBar::new(0);
    overall_pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    overall_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Removed {pos}/{len} packages",
    )?);
    overall_pb.enable_steady_tick(Duration::from_millis(120));

    let overall_pb_ref = overall_pb.clone();
    let mut overall_progress_callback = Some(move |done: u32, total: u32| {
        overall_pb_ref.set_length(total as u64);
        overall_pb_ref.set_position(done as u64);
    });

    let overall_pb_for_messages = overall_pb.clone();
    let mut message_callback = Some(move |msg: &str| {
        overall_pb_for_messages.println(msg);
    });

    if names.len() > 1 {
        let (removed, failed) = package_remover.remove_bulk(
            &names,
            &purge,
            &mut message_callback,
            &mut overall_progress_callback,
        )?;
        overall_pb.finish_and_clear();
        if failed > 0 {
            println!(
                "{}",
                output::warning(format!(
                    "Removal complete: {} removed, {} failed.",
                    removed, failed
                ))
            );
        } else {
            println!(
                "{}",
                output::success(format!("Removal complete: {} removed, 0 failed.", removed))
            );
        }
    } else {
        package_remover.remove_single(&names[0], &purge, &mut message_callback)?;
        overall_pb.finish_and_clear();
        println!(
            "{}",
            output::success("Removal complete: 1 removed, 0 failed.")
        );
    }

    Ok(())
}

fn run_dry_run(
    names: Vec<String>,
    purge: bool,
    package_remover: &mut RemoveOperation<'_>,
) -> Result<()> {
    println!("{}", output::title("Remove preview"));
    output::kv("Purge", if purge { "yes" } else { "no" });
    output::action_note("resolve only (no remove, no purge, no metadata changes)");
    println!();

    let mut message_callback = Some(|msg: &str| println!("{msg}"));
    if names.len() > 1 {
        let (planned, failed) =
            package_remover.preview_bulk(&names, &purge, &mut message_callback)?;
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
    println!();
    output::status_line(Status::Ok, "summary", "1 planned, 0 failed");
    Ok(())
}
