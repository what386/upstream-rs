use anyhow::{Result, anyhow};
use console::style;

use crate::{
    application::operations::{
        install_operation::InstallOperation, remove_operation::RemoveOperation,
    },
    models::upstream::{InstallType, Package},
    providers::provider_manager::ProviderManager,
    services::{
        builder::{BuildRequest, worker::BuildWorker},
        storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
    },
    utils::static_paths::UpstreamPaths,
};

pub async fn run(names: Vec<String>, ignore_checksums: bool) -> Result<()> {
    if names.is_empty() {
        return Err(anyhow!("At least one package name is required"));
    }

    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let github_token = config.get_config().github.api_token.as_deref();
    let gitlab_token = config.get_config().gitlab.api_token.as_deref();
    let gitea_token = config.get_config().gitea.api_token.as_deref();
    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;

    let mut reinstalled = 0_u32;
    let mut failed = 0_u32;

    for name in &names {
        println!("{}", style(format!("Reinstalling '{}' ...", name)).cyan());
        let mut msg = Some(|line: &str| println!("{line}"));

        let package = match package_storage.get_package_by_name(name) {
            Some(pkg) => pkg.clone(),
            None => {
                println!(
                    "{}",
                    style(format!(
                        "Reinstall failed: package '{}' is not installed",
                        name
                    ))
                    .red()
                );
                failed += 1;
                continue;
            }
        };

        if let Err(err) = reinstall_one(
            &provider_manager,
            &mut package_storage,
            &paths,
            package,
            ignore_checksums,
            &mut msg,
        )
        .await
        {
            println!("{}", style(format!("Reinstall failed: {}", err)).red());
            failed += 1;
            continue;
        }

        println!("{}", style("Package reinstalled").green());
        reinstalled += 1;
    }

    if names.len() == 1 {
        if failed == 0 {
            println!(
                "{}",
                style("Reinstall complete: 1 reinstalled, 0 failed.").green()
            );
            return Ok(());
        }
        return Err(anyhow!("Reinstall failed"));
    }

    if failed > 0 {
        println!(
            "{}",
            style(format!(
                "Reinstall complete: {} reinstalled, {} failed.",
                reinstalled, failed
            ))
            .yellow()
        );
    } else {
        println!(
            "{}",
            style(format!(
                "Reinstall complete: {} reinstalled, 0 failed.",
                reinstalled
            ))
            .green()
        );
    }

    Ok(())
}

async fn reinstall_one<H>(
    provider_manager: &ProviderManager,
    package_storage: &mut PackageStorage,
    paths: &UpstreamPaths,
    package: Package,
    ignore_checksums: bool,
    message_callback: &mut Option<H>,
) -> Result<()>
where
    H: FnMut(&str),
{
    let had_icon = package.icon_path.is_some();
    let version_tag = format!("v{}", package.version);
    let mut reinstall_package = package.clone();
    reinstall_package.install_path = None;
    reinstall_package.exec_path = None;
    reinstall_package.icon_path = None;

    let mut remove_op = RemoveOperation::new(package_storage, paths);
    remove_op.remove_single(&package.name, &false, message_callback)?;

    match package.install_type {
        InstallType::Release => {
            let mut install_op = InstallOperation::new(provider_manager, package_storage, paths)?;
            install_op
                .install_single(
                    reinstall_package,
                    &Some(version_tag),
                    &had_icon,
                    ignore_checksums,
                    &mut None::<fn(u64, u64)>,
                    message_callback,
                )
                .await?;
        }
        InstallType::Build => {
            let worker = BuildWorker::new(provider_manager);
            let output = worker
                .build(
                    BuildRequest {
                        name: reinstall_package.name.clone(),
                        repo_slug: reinstall_package.repo_slug.clone(),
                        provider: reinstall_package.provider.clone(),
                        base_url: reinstall_package.base_url.clone(),
                        version_tag: Some(version_tag),
                        requested_profile: None,
                        build_output: None,
                    },
                    reinstall_package.channel.clone(),
                )
                .await?;

            let mut install_op = InstallOperation::new(provider_manager, package_storage, paths)?;
            install_op
                .install_local_artifact(
                    reinstall_package,
                    &output.artifact_path,
                    output.version,
                    &had_icon,
                    message_callback,
                )
                .await?;
        }
    }

    Ok(())
}
