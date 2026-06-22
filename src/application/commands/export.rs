use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

use crate::{
    application::operations::export_op::ExportOperation, output,
    services::packaging::OperationProgressEvent, storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

fn render_export_progress(event: OperationProgressEvent) -> String {
    match event {
        OperationProgressEvent::Phase(phase) => phase.label().to_string(),
        OperationProgressEvent::Count { done, total } => format!("Exporting ... {done}/{total}"),
        OperationProgressEvent::Warning(message) | OperationProgressEvent::Detail(message) => {
            message
        }
    }
}

pub async fn run_export(path: PathBuf, full: bool) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let export_op = ExportOperation::new(&package_database, &paths);

    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Exporting ...");

    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_export_progress(event));
    });

    if full {
        println!("{}", output::title("Export snapshot"));
        output::action_note(format!("Destination: {}", path.display()));

        export_op.export_snapshot(&path, &mut progress_callback)?;

        pb.finish_and_clear();
        println!(
            "{}",
            output::success(format!("Snapshot complete: saved to '{}'.", path.display()))
        );
    } else {
        println!("{}", output::title("Export manifest"));
        output::action_note(format!("Destination: {}", path.display()));

        export_op.export_manifest(&path, &mut progress_callback)?;

        pb.finish_and_clear();
        println!(
            "{}",
            output::success(format!("Manifest complete: saved to '{}'.", path.display()))
        );
    }

    Ok(())
}
