use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::operations::remove_operation::RemoveOperation,
    services::storage::package_storage::PackageStorage, utils::static_paths::UpstreamPaths,
};

pub fn run(names: Vec<String>, purge: bool) -> Result<()> {
    let paths = UpstreamPaths::new();

    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let mut package_remover = RemoveOperation::new(&mut package_storage, &paths);

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
                style(format!(
                    "Removal complete: {} removed, {} failed.",
                    removed, failed
                ))
                .yellow()
            );
        } else {
            println!(
                "{}",
                style(format!("Removed {} package(s).", removed)).green()
            );
        }
    } else {
        package_remover.remove_single(&names[0], &purge, &mut message_callback)?;
        overall_pb.finish_and_clear();
        println!("{}", style("Package removed!").green());
    }

    Ok(())
}
