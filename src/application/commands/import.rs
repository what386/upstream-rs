use crate::{
    application::operations::import_operation::{ImportKind, ImportOperation},
    output,
    services::{packaging::OperationProgressEvent, storage::package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKindArg {
    Keys,
    Manifest,
    Snapshot,
}

impl From<ImportKindArg> for ImportKind {
    fn from(value: ImportKindArg) -> Self {
        match value {
            ImportKindArg::Keys => ImportKind::Keys,
            ImportKindArg::Manifest => ImportKind::Manifest,
            ImportKindArg::Snapshot => ImportKind::Snapshot,
        }
    }
}

fn render_import_progress(event: OperationProgressEvent) -> String {
    match event {
        OperationProgressEvent::Phase(phase) => phase.label().to_string(),
        OperationProgressEvent::Count { done, total } => format!("Importing ... {done}/{total}"),
        OperationProgressEvent::Warning(message) | OperationProgressEvent::Detail(message) => {
            message
        }
    }
}

pub async fn run_import(
    path: PathBuf,
    skip_failed: bool,
    import_as: Option<ImportKindArg>,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut import_op = ImportOperation::new(&mut package_storage, &paths);

    println!("{}", output::title("Import"));
    output::action_note(format!("Source: {}", path.display()));

    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Importing ...");

    let progress_pb = pb.clone();
    let mut progress_callback = Some(move |event: OperationProgressEvent| {
        progress_pb.set_message(render_import_progress(event));
    });

    import_op
        .import(
            &path,
            skip_failed,
            import_as.map(Into::into),
            &mut progress_callback,
        )
        .await?;

    pb.finish_and_clear();
    println!("{}", output::success("Import complete."));

    Ok(())
}
