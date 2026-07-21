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

pub fn run_export_packages(path: PathBuf, paths: &UpstreamPaths) -> Result<()> {
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let export_op = ExportOperation::new(&package_database, paths);
    let pb = new_export_progress_bar();
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_export_progress(event));
    });

    println!("{}", output::title("Export packages"));
    output::action_note(format!("Destination: {}", path.display()));
    export_op.export_packages(&path, &mut progress_callback)?;

    pb.finish_and_clear();
    println!(
        "{}",
        output::success(format!("Packages complete: saved to '{}'.", path.display()))
    );
    Ok(())
}

pub fn run_export_keys(path: PathBuf, paths: &UpstreamPaths) -> Result<()> {
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let export_op = ExportOperation::new(&package_database, paths);
    let pb = new_export_progress_bar();
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_export_progress(event));
    });

    println!("{}", output::title("Export keys"));
    output::action_note(format!("Destination: {}", path.display()));
    export_op.export_keys(&path, &mut progress_callback)?;

    pb.finish_and_clear();
    println!(
        "{}",
        output::success(format!("Keys complete: saved to '{}'.", path.display()))
    );
    Ok(())
}

pub fn run_export_config(path: PathBuf, paths: &UpstreamPaths) -> Result<()> {
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let export_op = ExportOperation::new(&package_database, paths);
    let pb = new_export_progress_bar();
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_export_progress(event));
    });

    println!("{}", output::title("Export config"));
    output::action_note(format!("Destination: {}", path.display()));
    export_op.export_config(&path, &mut progress_callback)?;

    pb.finish_and_clear();
    println!(
        "{}",
        output::success(format!("Config complete: saved to '{}'.", path.display()))
    );
    Ok(())
}

pub fn run_export_profile(path: PathBuf, paths: &UpstreamPaths) -> Result<()> {
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let export_op = ExportOperation::new(&package_database, paths);
    let pb = new_export_progress_bar();
    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_export_progress(event));
    });

    println!("{}", output::title("Export profile"));
    output::action_note(format!("Destination: {}", path.display()));
    export_op.export_profile(&path, &mut progress_callback)?;

    pb.finish_and_clear();
    println!(
        "{}",
        output::success(format!("Profile complete: saved to '{}'.", path.display()))
    );
    Ok(())
}

fn new_export_progress_bar() -> ProgressBar {
    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .expect("valid export progress style"),
    );
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Exporting ...");
    pb
}
