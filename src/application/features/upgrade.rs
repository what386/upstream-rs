use anyhow::Result;

use crate::{
    application::operations::package_upgrade::PackageUpgrader,
    services::{
        providers::provider_manager::ProviderManager,
        storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    },
    utils::static_paths::UpstreamPaths,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub async fn run(names: Option<Vec<String>>, force_option: bool, check_option: bool) -> Result<()> {
    let paths = UpstreamPaths::new();
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let github_token = config.get_config().github.api_token.as_deref();
    let provider_manager = ProviderManager::new(github_token)?;
    let mut package_upgrade =
        PackageUpgrader::new(&provider_manager, &mut package_storage, &paths)?;

    // Handle --check flag
    if check_option {
        return run_check(package_upgrade, names).await;
    }

    // Normal upgrade flow
    let mp = MultiProgress::new();
    let overall_pb = mp.add(ProgressBar::new(0));
    overall_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} Upgraded {pos}/{len} packages",
    )?);

    let overall_pb_ref = overall_pb.clone();
    let mut overall_progress_callback = Some(move |done: u32, total: u32| {
        overall_pb_ref.set_length(total as u64);
        overall_pb_ref.set_position(done as u64);
    });

    let download_pb = mp.add(ProgressBar::new(0));
    download_pb.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
    )?);

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
        package_upgrade
            .upgrade_all(
                &force_option,
                &mut download_progress_callback,
                &mut overall_progress_callback,
                &mut message_callback,
            )
            .await?;

        download_pb.finish_and_clear();
        overall_pb.finish_with_message("Upgrade complete!");
        return Ok(());
    }

    let name_vec = names.unwrap();
    if name_vec.len() > 1 {
        package_upgrade
            .upgrade_bulk(
                &name_vec,
                &force_option,
                &mut download_progress_callback,
                &mut overall_progress_callback,
                &mut message_callback,
            )
            .await?;
    } else {
        package_upgrade
            .upgrade_single(
                &name_vec[0],
                &force_option,
                &mut download_progress_callback,
                &mut message_callback,
            )
            .await?;
    }

    download_pb.finish_and_clear();
    overall_pb.finish_with_message("Upgrade complete!");
    Ok(())
}

// TODO: make update messages mutate in-place
// e.g. "checking xyz... -> xyz is up to date!"
// instead of "checking xyz... -> checking xyz...
//                                xyz is up to date!"
// maybe use a spinner, too?
async fn run_check(
    package_upgrade: PackageUpgrader<'_>,
    names: Option<Vec<String>>,
) -> Result<()> {
    let mut message_callback = Some(|msg: &str| {
        println!("{}", msg);
    });

    match names {
        // Check all packages
        None => {
            println!("Checking for updates...\n");
            let updates = package_upgrade
                .check_updates(&mut message_callback)
                .await?;

            if updates.is_empty() {
                println!("\n✓ All packages are up to date!");
            } else {
                println!("\n{} update(s) available:\n", updates.len());
                for (name, current, latest) in updates {
                    println!("  {} {} → {}", name, current, latest);
                }
            }
        }

        // Check specific package(s)
        Some(name_vec) => {
            if name_vec.len() == 1 {
                // Single package check
                let package_name = &name_vec[0];
                match package_upgrade
                    .check_single_update(package_name, &mut message_callback)
                    .await?
                {
                    Some((current, latest)) => {
                        println!("\n✓ Update available for '{}':", package_name);
                        println!("  {} → {}", current, latest);
                    }
                    None => {
                        println!("\n✓ '{}' is up to date", package_name);
                    }
                }
            } else {
                // Multiple packages check
                println!("Checking specified packages...\n");
                let mut updates_found = Vec::new();
                let mut up_to_date = Vec::new();
                let mut not_found = Vec::new();

                for name in &name_vec {
                    match package_upgrade
                        .check_single_update(name, &mut message_callback)
                        .await
                    {
                        Ok(Some((current, latest))) => {
                            updates_found.push((name.clone(), current, latest));
                        }
                        Ok(None) => {
                            up_to_date.push(name.clone());
                        }
                        Err(_) => {
                            not_found.push(name.clone());
                        }
                    }
                }

                // Print results
                if !updates_found.is_empty() {
                    println!("\n{} update(s) available:\n", updates_found.len());
                    for (name, current, latest) in updates_found {
                        println!("  {} {} → {}", name, current, latest);
                    }
                }

                if !up_to_date.is_empty() {
                    println!("\n{} package(s) up to date:", up_to_date.len());
                    for name in up_to_date {
                        println!("  ✓ {}", name);
                    }
                }

                if !not_found.is_empty() {
                    println!("\n{} package(s) not found:", not_found.len());
                    for name in not_found {
                        println!("  ✗ {}", name);
                    }
                }
            }
        }
    }

    Ok(())
}
