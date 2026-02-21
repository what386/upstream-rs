use crate::{
    application::operations::import_operation::ImportOperation,
    providers::provider_manager::ProviderManager,
    services::storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

pub async fn run_import(path: PathBuf, skip_failed: bool) -> Result<()> {
    let paths = UpstreamPaths::new();
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let github_token = config.get_config().github.api_token.as_deref();
    let gitlab_token = config.get_config().gitlab.api_token.as_deref();
    let gitea_token = config.get_config().gitea.api_token.as_deref();
    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token, None)?;

    let mut import_op = ImportOperation::new(&provider_manager, &mut package_storage, &paths);

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
