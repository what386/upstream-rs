use anyhow::{Result, anyhow};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::operations::{
        install_operation::InstallOperation, remove_operation::RemoveOperation,
    },
    models::{
        common::enums::TrustMode,
        upstream::{InstallType, Package},
    },
    output::{self, SizeImpactRow, Status},
    providers::provider_manager::ProviderManager,
    services::{
        builder::scripts::BuildScriptAction,
        builder::{BuildRequest, worker::BuildWorker},
        packaging::{
            PackageRemover,
            disk_impact::{
                ByteEstimate, DiskImpact, SignedByteEstimate, asset_size_estimate,
                estimate_path_size, install_impact_from_download,
            },
        },
        storage::{
            config_storage::ConfigStorage, metadata_storage::MetadataStorage,
            package_storage::PackageStorage, rollback_storage::RollbackSource,
        },
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

fn reinstall_phase_label(message: &str) -> String {
    if message.starts_with("Removing") {
        "Removing current install ...".to_string()
    } else if message.starts_with("Installing") || message.starts_with("Extracting") {
        "Installing package ...".to_string()
    } else if message.starts_with("Searching for executable") {
        "Resolving executable ...".to_string()
    } else if message.starts_with("Added '") && message.contains("' to PATH") {
        "Updating PATH ...".to_string()
    } else if message.starts_with("Creating symlink") || message.starts_with("Updating symlink") {
        "Creating runtime links ...".to_string()
    } else if message.starts_with("Saving package metadata") {
        "Saving metadata ...".to_string()
    } else if message.contains("source")
        || message.starts_with("Fetching ")
        || message.starts_with("Downloading ")
        || message.starts_with("Unpacking ")
        || message.starts_with("Resolving ")
        || message.starts_with("Detecting ")
        || message.starts_with("Building ")
        || message.starts_with("Running ")
        || message.starts_with("Staging ")
    {
        message.to_string()
    } else {
        format!("Building package ... {message}")
    }
}

pub async fn run(
    names: Vec<String>,
    trust_mode: TrustMode,
    force: bool,
    dry_run: bool,
) -> Result<()> {
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
    let trusted_keys = app_config.trusted_signature_keys();

    if dry_run {
        return run_dry_run(
            names,
            trust_mode,
            &mut package_storage,
            &provider_manager,
            &paths,
        )
        .await;
    }

    let impact =
        estimate_reinstall_impact(&names, &package_storage, &provider_manager, &paths).await;
    let rollback_impact = estimate_reinstall_rollback_impact(&names, &package_storage, &paths);
    let size_rows = rollback_size_rows(rollback_impact);
    output::print_disk_impact_with_size_rows(&impact, &size_rows, true);
    output::confirm_or_cancel(format!("Reinstall {} package(s)?", names.len()), false)?;

    let mut reinstalled = 0_u32;
    let mut failed = 0_u32;
    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Reinstalling packages ...");
    let mut completion_lines = Vec::new();

    for name in &names {
        let package_name = name.clone();
        let progress_pb = pb.clone();
        let mut msg = Some(move |line: &str| {
            progress_pb.set_message(format!(
                "Reinstalling {package_name}\n {:<28} {}",
                package_name,
                reinstall_phase_label(line)
            ));
        });

        let package = match package_storage.get_package_by_name(name) {
            Some(pkg) => pkg.clone(),
            None => {
                completion_lines.push(output::status_line_text(
                    Status::Fail,
                    name,
                    "package is not installed",
                ));
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
            force,
            &trusted_keys,
            &mut msg,
        )
        .await
        {
            completion_lines.push(output::status_line_text(Status::Fail, name, err));
            failed += 1;
            continue;
        }

        completion_lines.push(output::status_line_text(Status::Ok, name, "reinstalled"));
        reinstalled += 1;
    }
    pb.finish_and_clear();
    for line in completion_lines {
        println!("{line}");
    }

    if names.len() == 1 {
        if failed == 0 {
            println!(
                "{}",
                output::success("Reinstall complete: 1 reinstalled, 0 failed.")
            );
            return Ok(());
        }
        return Err(anyhow!("Reinstall failed"));
    }

    if failed > 0 {
        println!(
            "{}",
            output::warning(format!(
                "Reinstall complete: {} reinstalled, {} failed.",
                reinstalled, failed
            ))
        );
    } else {
        println!(
            "{}",
            output::success(format!(
                "Reinstall complete: {} reinstalled, 0 failed.",
                reinstalled
            ))
        );
    }

    Ok(())
}

async fn run_dry_run(
    names: Vec<String>,
    trust_mode: TrustMode,
    package_storage: &mut PackageStorage,
    provider_manager: &ProviderManager,
    paths: &UpstreamPaths,
) -> Result<()> {
    println!("{}", output::title("Reinstall preview"));
    output::kv("Trust", trust_mode);
    let impact = estimate_reinstall_impact(&names, package_storage, provider_manager, paths).await;
    let rollback_impact = estimate_reinstall_rollback_impact(&names, package_storage, paths);
    let size_rows = rollback_size_rows(rollback_impact);
    output::print_disk_impact_with_size_rows(&impact, &size_rows, true);
    output::action_note(
        "resolve only (no remove, no download, no build, no install, no metadata changes)",
    );
    println!();

    let mut planned = 0_u32;
    let mut failed = 0_u32;

    for name in &names {
        let Some(package) = package_storage.get_package_by_name(name).cloned() else {
            output::status_line(Status::Fail, name, "not installed");
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
                    .get_release_by_tag(
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
                                let resolved_filetype = if preview_package.filetype
                                    == crate::models::common::enums::Filetype::Auto
                                {
                                    asset.filetype
                                } else {
                                    preview_package.filetype
                                };
                                output::status_line(
                                    Status::Plan,
                                    &package.name,
                                    format!(
                                        "reinstall release {} ({}) asset {} ({:?})",
                                        release.name, release.tag, asset.name, resolved_filetype
                                    ),
                                );
                                output::action_note(format!(
                                    "{:<28} remove/install runtime files",
                                    package.name
                                ));
                                planned += 1;
                            }
                            Err(err) => {
                                output::status_line(
                                    Status::Fail,
                                    &package.name,
                                    format!("failed to select release asset {version_tag}: {err}"),
                                );
                                failed += 1;
                            }
                        }
                    }
                    Err(err) => {
                        output::status_line(
                            Status::Fail,
                            &package.name,
                            format!("failed to resolve release {version_tag}: {err}"),
                        );
                        failed += 1;
                    }
                }
            }
            InstallType::Build => {
                if let Some(branch) = package.build_branch.clone() {
                    match provider_manager
                        .get_branch_head_sha(
                            &package.repo_slug,
                            &package.provider,
                            &branch,
                            package.base_url.as_deref(),
                        )
                        .await
                    {
                        Ok(commit) => {
                            output::status_line(
                                Status::Plan,
                                &package.name,
                                format!(
                                    "rebuild {} ({}) branch {} @ {}",
                                    package.repo_slug, package.provider, branch, commit
                                ),
                            );
                            output::action_note(format!(
                                "{:<28} remove/install runtime files",
                                package.name
                            ));
                            planned += 1;
                        }
                        Err(err) => {
                            output::status_line(
                                Status::Fail,
                                &package.name,
                                format!("failed to resolve build branch {branch}: {err}"),
                            );
                            failed += 1;
                        }
                    }
                } else {
                    let version_tag = format!("v{}", package.version);
                    match provider_manager
                        .get_release_by_tag(
                            &package.repo_slug,
                            &version_tag,
                            &package.provider,
                            package.base_url.as_deref(),
                        )
                        .await
                    {
                        Ok(release) => {
                            output::status_line(
                                Status::Plan,
                                &package.name,
                                format!(
                                    "rebuild {} ({}) release {} ({})",
                                    package.repo_slug, package.provider, release.name, release.tag
                                ),
                            );
                            output::action_note(format!(
                                "{:<28} remove/install runtime files",
                                package.name
                            ));
                            planned += 1;
                        }
                        Err(err) => {
                            output::status_line(
                                Status::Fail,
                                &package.name,
                                format!("failed to resolve release {version_tag}: {err}"),
                            );
                            failed += 1;
                        }
                    }
                }
            }
        }
    }

    println!();
    let status = if failed > 0 { Status::Warn } else { Status::Ok };
    output::status_line(
        status,
        "summary",
        format!("{planned} planned, {failed} failed"),
    );
    Ok(())
}

async fn estimate_reinstall_impact(
    names: &[String],
    package_storage: &PackageStorage,
    provider_manager: &ProviderManager,
    paths: &UpstreamPaths,
) -> DiskImpact {
    let mut total = DiskImpact::empty();
    let remover = PackageRemover::new(paths);

    for name in names {
        let Some(package) = package_storage.get_package_by_name(name) else {
            total = total + DiskImpact::unknown();
            continue;
        };

        let active_size = remover.estimate_active_size(package).unwrap_or(0);
        let new_install = match package.install_type {
            InstallType::Release => {
                let mut preview_package = package.clone();
                preview_package.install_path = None;
                preview_package.exec_path = None;
                preview_package.icon_path = None;
                let version_tag = format!("v{}", package.version);
                match provider_manager
                    .get_release_by_tag(
                        &preview_package.repo_slug,
                        &version_tag,
                        &preview_package.provider,
                        preview_package.base_url.as_deref(),
                    )
                    .await
                    .and_then(|release| {
                        provider_manager.find_recommended_asset(&release, &preview_package)
                    }) {
                    Ok(asset) => install_impact_from_download(asset_size_estimate(asset.size)),
                    Err(_) => DiskImpact::unknown(),
                }
            }
            InstallType::Build => DiskImpact::unknown(),
        };

        let package_impact = if let Some(new_bytes) = new_install.net.bytes {
            DiskImpact {
                download: new_install.download,
                net: SignedByteEstimate::estimated(
                    new_bytes.saturating_sub(i128::from(active_size)),
                ),
            }
        } else {
            DiskImpact {
                download: ByteEstimate::unknown(),
                net: SignedByteEstimate::unknown(),
            }
        };
        total = total + package_impact;
    }

    total
}

fn estimate_reinstall_rollback_impact(
    names: &[String],
    package_storage: &PackageStorage,
    paths: &UpstreamPaths,
) -> SignedByteEstimate {
    let remover = PackageRemover::new(paths);
    names
        .iter()
        .map(|name| {
            let Some(package) = package_storage.get_package_by_name(name) else {
                return SignedByteEstimate::unknown();
            };
            let active_size = remover.estimate_active_size(package).unwrap_or(0);
            let existing_rollback =
                estimate_path_size(&paths.install.rollback_dir.join(&package.name)).unwrap_or(0);
            SignedByteEstimate::exact(
                i128::from(active_size).saturating_sub(i128::from(existing_rollback)),
            )
        })
        .fold(SignedByteEstimate::exact(0), |total, impact| total + impact)
}

fn rollback_size_rows(rollback_impact: SignedByteEstimate) -> Vec<SizeImpactRow> {
    if matches!(rollback_impact.bytes, Some(0)) {
        Vec::new()
    } else {
        vec![SizeImpactRow::new("Rollback storage", rollback_impact)]
    }
}

#[allow(clippy::too_many_arguments)]
async fn reinstall_one<H>(
    provider_manager: &ProviderManager,
    package_storage: &mut PackageStorage,
    metadata_storage: &mut MetadataStorage,
    paths: &UpstreamPaths,
    package: Package,
    trust_mode: TrustMode,
    force: bool,
    trusted_keys: &TrustedSignatureKeys,
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
    let mut no_remove_progress =
        Some(|_: &str, _: crate::services::packaging::PackageProgressEvent| {});
    remove_op.remove_single_with_source(
        &package.name,
        &false,
        &force,
        RollbackSource::Reinstall,
        message_callback,
        &mut no_remove_progress,
    )?;

    match package.install_type {
        InstallType::Release => {
            let mut install_op = InstallOperation::new(
                provider_manager,
                package_storage,
                paths,
                trusted_keys.clone(),
            )?;
            install_op
                .install_single(
                    reinstall_package,
                    &Some(version_tag),
                    &had_icon,
                    trust_mode,
                    &mut Some(|_: u64, _: u64| {}),
                    message_callback,
                )
                .await?;
        }
        InstallType::Build => {
            let worker = BuildWorker::new(provider_manager);
            let output = {
                let mut build_line_callback = Some(|line: &str| {
                    if let Some(callback) = message_callback.as_mut() {
                        callback(line);
                    }
                });
                worker
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
                            script_action: BuildScriptAction::Upgrade,
                        },
                        reinstall_package.channel.clone(),
                        &mut build_line_callback,
                    )
                    .await?
            };
            reinstall_package.build_branch = output.branch.clone();
            reinstall_package.build_commit = output.commit.clone();

            let mut install_op = InstallOperation::new(
                provider_manager,
                package_storage,
                paths,
                trusted_keys.clone(),
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
