use crate::{
    application::operations::import_operation::{ImportKind, ImportOperation},
    services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use console::style;
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

pub async fn run_import(
    path: PathBuf,
    skip_failed: bool,
    import_as: Option<ImportKindArg>,
    yes: bool,
) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut import_op = ImportOperation::new(&mut package_storage, &paths);

    println!(
        "{}",
        style(format!("Importing from '{}' ...", path.display())).cyan()
    );

    let pb = ProgressBar::new(0);
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )?);
    pb.enable_steady_tick(Duration::from_millis(120));

    let pb_ref = &pb;
    let mut download_progress_callback = Some(move |downloaded: u64, total: u64| {
        pb_ref.set_length(total);
        pb_ref.set_position(downloaded);
    });

    let mut overall_progress_callback: Option<Box<dyn FnMut(u32, u32)>> = None;

    let mut message_callback = Some(move |msg: &str| {
        pb_ref.println(msg);
    });

    import_op
        .import(
            &path,
            skip_failed,
            import_as.map(Into::into),
            yes,
            &mut download_progress_callback,
            &mut overall_progress_callback,
            &mut message_callback,
        )
        .await?;

    pb.set_position(pb.length().unwrap_or(0));
    pb.finish_with_message("Import complete");
    println!("{}", style("Import complete.").green());

    Ok(())
}
