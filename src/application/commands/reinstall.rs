use anyhow::{Result, anyhow};
use console::style;

use crate::{
    application::operations::{
        install_operation::InstallOperation, remove_operation::RemoveOperation,
    },
    models::{
        common::enums::TrustMode,
        upstream::{InstallType, Package},
    },
    providers::provider_manager::ProviderManager,
    services::{
        builder::{BuildRequest, worker::BuildWorker},
        storage::{
            config_storage::ConfigStorage, metadata_storage::MetadataStorage,
            package_storage::PackageStorage, rollback_storage::RollbackSource,
        },
        trust::MinisignPublicKey,
    },
    utils::static_paths::UpstreamPaths,
};

pub async fn run(names: Vec<String>, trust_mode: TrustMode, dry_run: bool) -> Result<()> {
    if names.is_empty() {
        return Err(anyhow!("At least one package name is required"));
    }

    let paths = UpstreamPaths::new()?;
    let config = ConfigStorage::new(&paths.config.config_file)?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let app_config = config.get_config();

    let github_token = app_config.github.api_token.as_deref();
    let gitlab_token = app_config.gitlab.api_token.as_deref();
    let gitea_token = app_config.gitea.api_token.as_deref();
    let provider_manager = ProviderManager::new(github_token, gitlab_token, gitea_token)?;
    let trusted_keys = app_config.trusted_minisign_keys();

    if dry_run {
        return run_dry_run(names, trust_mode, &mut package_storage, &provider_manager).await;
    }

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
            &mut metadata_storage,
            &paths,
            package,
            trust_mode,
            &trusted_keys,
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

async fn run_dry_run(
    names: Vec<String>,
    trust_mode: TrustMode,
    package_storage: &mut PackageStorage,
    provider_manager: &ProviderManager,
) -> Result<()> {
    println!("{}", style("Dry run: reinstall preview").bold());
    println!("  trust mode: {}", trust_mode);

    let mut planned = 0_u32;
    let mut failed = 0_u32;

    for name in &names {
        let Some(package) = package_storage.get_package_by_name(name).cloned() else {
            println!("{:<7} {:<28} not installed", "[x]", name);
            failed += 1;
            continue;
        };

        match package.install_type {
            InstallType::Release => {
                let mut preview_package = package.clone();
                preview_package.install_path = None;
                preview_package.exec_path = None;
                preview_package.icon_path = None;
                let version_tag = format!("v{}", package.version);

                match provider_manager
                    .get_release_by_tag_for(
                        &preview_package.repo_slug,
                        &version_tag,
                        &preview_package.provider,
                        preview_package.base_url.as_deref(),
                    )
                    .await
                {
                    Ok(release) => {
                        match provider_manager.find_recommended_asset(&release, &preview_package) {
                            Ok(asset) => {
                                println!(
                                    "{:<7} {:<28} would reinstall release {} ({}) asset {} ({:?})",
                                    "[plan]",
                                    package.name,
                                    release.name,
                                    release.tag,
                                    asset.name,
                                    if preview_package.filetype
                                        == crate::models::common::enums::Filetype::Auto
                                    {
                                        asset.filetype
                                    } else {
                                        preview_package.filetype
                                    }
                                );
                                println!(
                                    "        {:<28} would remove/install runtime files",
                                    package.name
                                );
                                planned += 1;
                            }
                            Err(err) => {
                                println!(
                                    "{:<7} {:<28} failed to select release asset {}: {}",
                                    "[!]", package.name, version_tag, err
                                );
                                failed += 1;
                            }
                        }
                    }
                    Err(err) => {
                        println!(
                            "{:<7} {:<28} failed to resolve release {}: {}",
                            "[!]", package.name, version_tag, err
                        );
                        failed += 1;
                    }
                }
            }
            InstallType::Build => {
                if let Some(branch) = package.build_branch.clone() {
                    match provider_manager
                        .get_branch_head_sha_for(
                            &package.repo_slug,
                            &package.provider,
                            &branch,
                            package.base_url.as_deref(),
                        )
                        .await
                    {
                        Ok(commit) => {
                            println!(
                                "{:<7} {:<28} would rebuild {} ({}) branch {} @ {}",
                                "[plan]",
                                package.name,
                                package.repo_slug,
                                package.provider,
                                branch,
                                commit
                            );
                            println!(
                                "        {:<28} would remove/install runtime files",
                                package.name
                            );
                            planned += 1;
                        }
                        Err(err) => {
                            println!(
                                "{:<7} {:<28} failed to resolve build branch {}: {}",
                                "[!]", package.name, branch, err
                            );
                            failed += 1;
                        }
                    }
                } else {
                    let version_tag = format!("v{}", package.version);
                    match provider_manager
                        .get_release_by_tag_for(
                            &package.repo_slug,
                            &version_tag,
                            &package.provider,
                            package.base_url.as_deref(),
                        )
                        .await
                    {
                        Ok(release) => {
                            println!(
                                "{:<7} {:<28} would rebuild {} ({}) release {} ({})",
                                "[plan]",
                                package.name,
                                package.repo_slug,
                                package.provider,
                                release.name,
                                release.tag
                            );
                            println!(
                                "        {:<28} would remove/install runtime files",
                                package.name
                            );
                            planned += 1;
                        }
                        Err(err) => {
                            println!(
                                "{:<7} {:<28} failed to resolve release {}: {}",
                                "[!]", package.name, version_tag, err
                            );
                            failed += 1;
                        }
                    }
                }
            }
        }
    }

    println!();
    println!("Dry run complete: {} planned, {} failed.", planned, failed);
    println!(
        "  actions: resolve only (no remove, no download, no build, no install, no metadata changes)"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn reinstall_one<H>(
    provider_manager: &ProviderManager,
    package_storage: &mut PackageStorage,
    metadata_storage: &mut MetadataStorage,
    paths: &UpstreamPaths,
    package: Package,
    trust_mode: TrustMode,
    trusted_keys: &[MinisignPublicKey],
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

    let mut remove_op = RemoveOperation::new(package_storage, metadata_storage, paths);
    remove_op.remove_single_with_source(
        &package.name,
        &false,
        RollbackSource::Reinstall,
        message_callback,
    )?;

    match package.install_type {
        InstallType::Release => {
            let mut install_op = InstallOperation::new(
                provider_manager,
                package_storage,
                paths,
                trusted_keys.to_vec(),
            )?;
            install_op
                .install_single(
                    reinstall_package,
                    &Some(version_tag),
                    &had_icon,
                    trust_mode,
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
                        version_tag: if reinstall_package.build_branch.is_some() {
                            None
                        } else {
                            Some(version_tag)
                        },
                        branch: reinstall_package.build_branch.clone(),
                        requested_profile: None,
                        build_output: None,
                    },
                    reinstall_package.channel.clone(),
                )
                .await?;
            reinstall_package.build_branch = output.branch.clone();
            reinstall_package.build_commit = output.commit.clone();

            let mut install_op = InstallOperation::new(
                provider_manager,
                package_storage,
                paths,
                trusted_keys.to_vec(),
            )?;
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
