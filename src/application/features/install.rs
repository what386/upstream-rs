use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use crate::{
    application::operations::package_install::PackageInstaller,
    models::{
        common::enums::{Channel, Filetype, Provider},
        upstream::Package,
    },
    services::{
        providers::provider_manager::ProviderManager,
        storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    },
    utils::static_paths::UpstreamPaths,
};

pub async fn run(
    repo_slug: String,
    provider: Provider,
    kind: Filetype,
    name: String,
    pattern: Option<String>,
    channel: Channel,
    create_entry: bool,
) -> Result<()> {
    println!("Installing {} from {} ...", name, provider);

    let paths = UpstreamPaths::new();

    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let github_token = config.get_config().github.api_token.as_deref();

    let provider_manager = ProviderManager::new(github_token)?;

    let mut package_installer =
        PackageInstaller::new(&provider_manager, &mut package_storage, &paths)?;

    let package = Package::with_defaults(name, repo_slug, kind, pattern, channel, provider);

    let pb = ProgressBar::new(0);
    pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )?);

    // Borrow pb for the closures
    let pb_ref = &pb;
    let mut download_progress_callback = Some(move |downloaded: u64, total: u64| {
        pb_ref.set_length(total);
        pb_ref.set_position(downloaded);
    });

    let mut message_callback = Some(move |msg: &str| {
        pb_ref.println(msg);
    });

    package_installer
        .install_single(
            package,
            &create_entry,
            &mut download_progress_callback,
            &mut message_callback,
        )
        .await?;

    // Set pb to 100%
    pb.set_position(pb.length().unwrap_or(0));

    pb.finish_with_message("Install complete");

    println!("{}", style("Install completed!").green());

    Ok(())
}
