use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

use crate::{
    application::operations::export_operation::ExportOperation,
    services::storage::package_storage::PackageStorage, utils::static_paths::UpstreamPaths,
};

pub async fn run_export(path: PathBuf, full: bool) -> Result<()> {
    let paths = UpstreamPaths::new();
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let export_op = ExportOperation::new(&package_storage, &paths);

    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template(
        "{msg}\n{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
    )?);
    pb.enable_steady_tick(Duration::from_millis(120));

    let pb_ref = &pb;

    let mut progress_callback = Some(move |done: u64, total: u64| {
        pb_ref.set_length(total);
        pb_ref.set_position(done);
    });

    let mut message_callback = Some(move |msg: &str| {
        pb_ref.set_message(msg.to_string());
    });

    if full {
        println!("{}", style("Creating full snapshot ...").cyan());

        export_op.export_snapshot(&path, &mut progress_callback, &mut message_callback)?;

        pb.set_position(pb.length().unwrap_or(0));
        pb.finish_with_message("Snapshot complete");

        println!(
            "{}",
            style(format!("Snapshot saved to '{}'", path.display())).green()
        );
    } else {
        println!("{}", style("Exporting package manifest ...").cyan());

        export_op.export_manifest(&path, &mut message_callback)?;

        pb.finish_with_message("Manifest complete");

        println!(
            "{}",
            style(format!("Manifest saved to '{}'", path.display())).green()
        );
    }

    Ok(())
}
