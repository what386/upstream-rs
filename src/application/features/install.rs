use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use crate::{
    application::operations::install_operation::InstallOperation,
    models::{
        common::enums::{Channel, Filetype, Provider},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    services::{
        storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    },
    utils::static_paths::UpstreamPaths,
};

pub async fn run(
    name: String,
    repo_slug: String,
    kind: Filetype,
    version: Option<String>,
    provider: Provider,
    channel: Channel,
    match_pattern: Option<String>,
    exclude_pattern: Option<String>,
    create_entry: bool,
) -> Result<()> {
    println!("Installing {} from {} ...", name, provider);

    let paths = UpstreamPaths::new();

    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let github_token = config.get_config().github.api_token.as_deref();

    let provider_manager = ProviderManager::new(github_token)?;

    let mut package_installer =
        InstallOperation::new(&provider_manager, &mut package_storage, &paths)?;

    let package = Package::with_defaults(name, repo_slug, kind, match_pattern, exclude_pattern, channel, provider);

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
            &version,
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
