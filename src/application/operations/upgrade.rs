use anyhow::{Ok, Result};

use crate::{
    application::{
        features::package_upgrader::PackageUpgrader,
    },
    services::{
        providers::provider_manager::ProviderManager,
        storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    },
    utils::static_paths::UpstreamPaths,
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub async fn run(
    names: Option<Vec<String>>,
    force_option: bool,
) -> Result<()> {
    // TODO: println for packages to upgrade

    let paths = UpstreamPaths::new();

    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let github_token = config.get_config().github.api_token.as_deref();

    let provider_manager = ProviderManager::new(github_token)?;

    let mut package_upgrader = PackageUpgrader::new(&provider_manager, &mut package_storage, &paths)?;

    let mp = MultiProgress::new();

    let overall_pb = mp.add(ProgressBar::new(0));
    overall_pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} Upgraded {pos}/{len} packages"
        )?
        .progress_chars("⠋⠙⠹⠸⠼⠴⠦⠧"),
    );

    let overall_pb_ref = overall_pb.clone();
    let mut overall_progress_callback = Some(move |done: u32, total: u32| {
        overall_pb_ref.set_length(total as u64);
        overall_pb_ref.set_position(done as u64);
    });

    let download_pb = mp.add(ProgressBar::new(0));
    download_pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})"
        )?
        .progress_chars("⠋⠙⠹⠸⠼⠴⠦⠧"),
    );

    let download_pb_ref = &download_pb;
    let mut download_progress_callback = Some(move |downloaded: u64, total: u64| {
        download_pb_ref.set_length(total);
        download_pb_ref.set_position(downloaded);
    });

    let message_pb = &overall_pb;
    let mut message_callback = Some(move |msg: &str| {
        message_pb.println(msg);
    });

    if names.is_none() {
        package_upgrader.upgrade_all(&force_option, &mut download_progress_callback, &mut overall_progress_callback, &mut message_callback).await?;

        // imagine the world if i could just use a goto
        download_pb.finish_and_clear();
        overall_pb.finish_with_message("Upgrade complete!");

        return Ok(());
    }

    let name_vec = names.unwrap();

    if name_vec.len() > 1 {
        package_upgrader.upgrade_bulk(&name_vec, &force_option, &mut download_progress_callback, &mut overall_progress_callback, &mut message_callback).await?;
    } else {
        package_upgrader.upgrade_single(&name_vec[0], &force_option, &mut download_progress_callback, &mut message_callback).await?;
    }

    download_pb.finish_and_clear();
    overall_pb.finish_with_message("Upgrade complete!");

    Ok(())
}
