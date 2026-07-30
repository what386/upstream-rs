use anyhow::{Context, Result, anyhow};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

use crate::{
    application::cancellation,
    application::context::CommandContext,
    models::{
        common::enums::TrustMode,
        provider::Release,
        upstream::{InstallType, Package, config::AppConfig},
    },
    output::{self, Status},
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{
            PackageInstaller, PackageProgressEvent, PackageRemover, PackageReplacer,
            PackageUpgrader, ResolvedUpgradeTarget,
            disk_impact::{
                ByteEstimate, DiskImpact, SignedByteEstimate, asset_size_estimate,
                install_impact_from_download,
            },
        },
        trust::TrustedSignatureKeys,
    },
    storage::database::PackageDatabase,
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
    trust_mode: Option<TrustMode>,
    force: bool,
    dry_run: bool,
    paths: &UpstreamPaths,
    app_config: &AppConfig,
) -> Result<()> {
    if names.is_empty() {
        return Err(anyhow!("At least one package name is required"));
    }

    let context = CommandContext::new(paths, app_config)?;
    let mut package_database = context.package_database()?;
    let trusted_keys = context.trusted_keys()?;

    if dry_run {
        return run_dry_run(
            names,
            trust_mode,
            &package_database,
            &context.provider_manager,
            context.paths,
        )
        .await;
    }

    let impact = estimate_reinstall_impact(
        &names,
        &package_database,
        &context.provider_manager,
        context.paths,
    )
    .await;
    output::print_disk_impact_with_size_rows(&impact, &[], true);
    output::confirm_or_cancel(format!("Reinstall {} package(s)?", names.len()), false)?;

    let mut reinstalled = 0_u32;
    let mut failed = 0_u32;
    let pb = ProgressBar::new_spinner();
    pb.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message("Reinstalling packages ...");
    let mut completion_lines = Vec::new();
    let completion_subject_width = output::status_subject_width(names.iter().map(String::as_str));

    for name in &names {
        cancellation::check()?;
        let package_name = name.clone();
        let progress_pb = pb.clone();
        let mut msg = Some(move |line: &str| {
            progress_pb.set_message(format!(
                "Reinstalling {package_name}\n {:<28} {}",
                package_name,
                reinstall_phase_label(line)
            ));
        });

        let package = match package_database.get_package(name)? {
            Some(pkg) => pkg,
            None => {
                completion_lines.push(output::status_line_text_with_width(
                    Status::Fail,
                    name,
                    "package is not installed",
                    completion_subject_width,
                ));
                failed += 1;
                continue;
            }
        };
        let effective_trust_mode = package_database.effective_trust_mode(name, trust_mode)?;

        if let Err(err) = reinstall_one(
            &context.provider_manager,
            &mut package_database,
            context.paths,
            package,
            effective_trust_mode,
            force,
            &trusted_keys,
            &mut msg,
        )
        .await
        {
            completion_lines.push(output::status_line_text_with_width(
                Status::Fail,
                name,
                output::error_summary(&err),
                completion_subject_width,
            ));
            failed += 1;
            continue;
        }

        completion_lines.push(output::status_line_text_with_width(
            Status::Ok,
            name,
            "reinstalled",
            completion_subject_width,
        ));
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

    if failed > 0 {
        return Err(anyhow!("{failed} package reinstall(s) failed"));
    }

    Ok(())
}

async fn run_dry_run(
    names: Vec<String>,
    trust_mode: Option<TrustMode>,
    package_database: &PackageDatabase,
    provider_manager: &ProviderManager,
    paths: &UpstreamPaths,
) -> Result<()> {
    println!("{}", output::title("Reinstall preview"));
    output::kv(
        "Trust",
        trust_mode
            .map(|mode| mode.to_string())
            .unwrap_or_else(|| "per-package (default: best-effort)".to_string()),
    );
    let impact = estimate_reinstall_impact(&names, package_database, provider_manager, paths).await;
    output::print_disk_impact_with_size_rows(&impact, &[], true);
    output::action_note(
        "resolve only (no remove, no download, no build, no install, no metadata changes)",
    );
    println!();

    let mut planned = 0_u32;
    let mut failed = 0_u32;

    for name in &names {
        let Some(package) = package_database.get_package(name)? else {
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

                match resolve_reinstall_release(provider_manager, &preview_package).await {
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
                                    format!(
                                        "failed to select release asset {}: {err}",
                                        release.tag
                                    ),
                                );
                                failed += 1;
                            }
                        }
                    }
                    Err(err) => {
                        output::status_line(
                            Status::Fail,
                            &package.name,
                            format!("failed to resolve release: {err}"),
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
                    match resolve_reinstall_release(provider_manager, &package).await {
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
                                format!("failed to resolve release: {err}"),
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
    if failed > 0 {
        return Err(anyhow!("{failed} package reinstall preview(s) failed"));
    }
    Ok(())
}

async fn estimate_reinstall_impact(
    names: &[String],
    package_database: &PackageDatabase,
    provider_manager: &ProviderManager,
    paths: &UpstreamPaths,
) -> DiskImpact {
    let mut total = DiskImpact::empty();
    let remover = PackageRemover::new(paths);

    for name in names {
        let Some(package) = package_database.get_package(name).ok().flatten() else {
            total = total + DiskImpact::unknown();
            continue;
        };

        let active_size = remover.estimate_active_size(&package).unwrap_or(0);
        let new_install = match package.install_type {
            InstallType::Release => {
                let mut preview_package = package.clone();
                preview_package.install_path = None;
                preview_package.exec_path = None;
                preview_package.icon_path = None;
                match resolve_reinstall_release(provider_manager, &preview_package)
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

async fn resolve_reinstall_release(
    provider_manager: &ProviderManager,
    package: &Package,
) -> Result<Release> {
    let tag = package.installed_release_tag().ok_or_else(|| {
        anyhow!(
            "package '{}' is missing its installed release tag; run `upstream upgrade {} --force` to repair package metadata",
            package.name,
            package.name
        )
    })?;

    provider_manager
        .get_release_by_tag(
            &package.repo_slug,
            &tag,
            &package.provider,
            package.base_url.as_deref(),
        )
        .await
        .map_err(|err| anyhow!("failed to resolve release tag '{}': {err}", tag))
}

#[allow(clippy::too_many_arguments)]
async fn reinstall_one<H>(
    provider_manager: &ProviderManager,
    package_database: &mut PackageDatabase,
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
    let target = match package.install_type {
        InstallType::Release => ResolvedUpgradeTarget::Release(
            resolve_reinstall_release(provider_manager, &package).await?,
        ),
        InstallType::Build if package.build_branch.is_none() => ResolvedUpgradeTarget::Release(
            resolve_reinstall_release(provider_manager, &package).await?,
        ),
        InstallType::Build => {
            let branch = package
                .build_branch
                .as_ref()
                .expect("build branch checked above")
                .clone();
            let head_commit = provider_manager
                .get_branch_head_sha(
                    &package.repo_slug,
                    &package.provider,
                    &branch,
                    package.base_url.as_deref(),
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to resolve branch '{}' for '{}'",
                        branch, package.name
                    )
                })?;
            ResolvedUpgradeTarget::Branch {
                branch,
                head_commit,
            }
        }
    };

    let installer = PackageInstaller::new(provider_manager, paths)?;
    let remover = PackageRemover::new(paths);
    let upgrader = PackageUpgrader::new(
        provider_manager,
        installer,
        remover,
        paths,
        trusted_keys.clone(),
    );
    let mut no_progress: Option<fn(PackageProgressEvent)> = None;
    let updated = upgrader
        .reinstall_resolved(
            &package,
            target,
            trust_mode,
            force,
            &mut Some(|_: u64, _: u64| {}),
            message_callback,
            &mut no_progress,
        )
        .await?;

    PackageReplacer::new(paths).commit(package_database, &updated)?;

    Ok(())
}
