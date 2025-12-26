use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use crate::{
    application::operations::package_remove::PackageRemover,
    services::storage::package_storage::PackageStorage, utils::static_paths::UpstreamPaths,
};

pub fn run(names: Vec<String>, purge: bool) -> Result<()> {
    let paths = UpstreamPaths::new();

    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let mut package_remover = PackageRemover::new(&mut package_storage, &paths);

    let overall_pb = ProgressBar::new(0);
    overall_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Removed {pos}/{len} packages",
    )?);

    let overall_pb_ref = overall_pb.clone();
    let mut overall_progress_callback = Some(move |done: u32, total: u32| {
        overall_pb_ref.set_length(total as u64);
        overall_pb_ref.set_position(done as u64);
    });

    let mut message_callback = Some(move |msg: &str| {
        overall_pb.println(msg);
    });

    if names.len() > 1 {
        package_remover.remove_bulk(
            &names,
            &purge,
            &mut message_callback,
            &mut overall_progress_callback,
        )?;
    } else {
        package_remover.remove_single(&names[0], &purge, &mut message_callback)?;
    }

    println!("{}", style("Package removed!").green());

    Ok(())
}
