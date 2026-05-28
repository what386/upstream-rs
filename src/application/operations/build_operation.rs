use anyhow::{Context, Result, anyhow, bail};

use crate::application::{operations::install_operation::InstallOperation, output};
use crate::models::{
    common::enums::{Channel, Filetype, Provider},
    upstream::{InstallType, Package},
};
use crate::providers::discovery::{SourceKind, infer_source};
use crate::providers::provider_manager::ProviderManager;
use crate::services::builder::{BuildProfile, BuildRequest, worker::BuildWorker};
use crate::services::packaging::disk_impact::{
    DiskImpact, asset_size_estimate, install_impact_from_download,
};
use crate::services::storage::package_storage::PackageStorage;
use crate::services::trust::TrustedSignatureKeys;
use crate::utils::static_paths::UpstreamPaths;

pub struct BuildOperation<'a> {
    provider_manager: &'a ProviderManager,
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
    trusted_keys: TrustedSignatureKeys,
}

pub struct BuildCommandInput {
    pub name: String,
    pub repo_slug: String,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub provider: Option<Provider>,
    pub base_url: Option<String>,
    pub channel: Channel,
    pub desktop: bool,
    pub build_profile: Option<BuildProfile>,
    pub build_output: Option<String>,
    pub dry_run: bool,
}

impl<'a> BuildOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
    ) -> Self {
        Self {
            provider_manager,
            package_storage,
            paths,
            trusted_keys,
        }
    }

    pub async fn build_and_install(&mut self, input: BuildCommandInput) -> Result<()> {
        let (resolved_repo_slug, resolved_provider, resolved_base_url) = if let Some(selected) =
            input.provider.as_ref()
        {
            (
                input.repo_slug.clone(),
                selected.clone(),
                input.base_url.clone(),
            )
        } else {
            let mut discovered = infer_source(&input.repo_slug)?;
            if let Some(base) = input.base_url.clone() {
                discovered.base_url = Some(base);
            }

            match discovered.kind {
                SourceKind::Repository | SourceKind::ForgeUrl => {}
                SourceKind::DirectAsset | SourceKind::DownloadPage => {
                    return Err(anyhow!(
                        "Build requires a forge repository source (github/gitlab/gitea), got '{}'",
                        input.repo_slug
                    ));
                }
            }

            (
                discovered.repo_slug,
                discovered.provider,
                discovered.base_url,
            )
        };

        if !matches!(
            resolved_provider,
            Provider::Github | Provider::Gitlab | Provider::Gitea
        ) {
            bail!("Build supports forge providers only (github/gitlab/gitea)");
        }

        if input.dry_run {
            let disk_impact = self
                .estimate_build_disk_impact(
                    &input,
                    &resolved_repo_slug,
                    &resolved_provider,
                    resolved_base_url.as_deref(),
                )
                .await;
            if let Some(branch) = input.branch.as_deref() {
                let commit = self
                    .provider_manager
                    .get_branch_head_sha(
                        &resolved_repo_slug,
                        &resolved_provider,
                        branch,
                        resolved_base_url.as_deref(),
                    )
                    .await
                    .context(format!(
                        "Failed to fetch branch head for '{}' on '{}'",
                        branch, resolved_repo_slug
                    ))?;
                println!("{}", output::title("Build preview"));
                output::kv("Package", &input.name);
                output::kv(
                    "Source",
                    format!("{} ({})", resolved_repo_slug, resolved_provider),
                );
                output::kv("Ref", format!("branch {} @ {}", branch, commit));
                output::print_transaction_table(
                    &[output::TransactionRow::single_version(
                        format!("{}/{}", resolved_provider, input.name),
                        branch,
                        disk_impact.net,
                        disk_impact.download,
                    )],
                    &disk_impact,
                    "Net disk change:",
                );
            } else {
                let release = if let Some(tag) = input.tag.as_deref() {
                    self.provider_manager
                        .get_release_by_tag(
                            &resolved_repo_slug,
                            tag,
                            &resolved_provider,
                            resolved_base_url.as_deref(),
                        )
                        .await
                        .context(format!(
                            "Failed to fetch release '{}' for '{}'",
                            tag, resolved_repo_slug
                        ))?
                } else {
                    self.provider_manager
                        .get_latest_release(
                            &resolved_repo_slug,
                            &resolved_provider,
                            &input.channel,
                            resolved_base_url.as_deref(),
                        )
                        .await
                        .context(format!(
                            "Failed to fetch latest release for '{}'",
                            resolved_repo_slug
                        ))?
                };
                println!("{}", output::title("Build preview"));
                output::kv("Package", &input.name);
                output::kv(
                    "Source",
                    format!("{} ({})", resolved_repo_slug, resolved_provider),
                );
                output::kv("Ref", format!("release {} ({})", release.name, release.tag));
                output::print_transaction_table(
                    &[output::TransactionRow::single_version(
                        format!("{}/{}", resolved_provider, input.name),
                        &release.tag,
                        disk_impact.net,
                        disk_impact.download,
                    )],
                    &disk_impact,
                    "Net disk change:",
                );
            }

            match input.build_profile {
                Some(profile) => output::kv("Profile", format!("{:?}", profile)),
                None => output::kv("Profile", "auto-detect at build time"),
            }
            if let Some(path) = input.build_output.as_deref() {
                output::kv("Build Output", path);
            } else {
                output::kv("Build Output", "none");
            }
            output::kv("Desktop", if input.desktop { "yes" } else { "no" });
            output::action_note("resolve only (no compile, no install, no metadata changes)");
            return Ok(());
        }

        let disk_impact = self
            .estimate_build_disk_impact(
                &input,
                &resolved_repo_slug,
                &resolved_provider,
                resolved_base_url.as_deref(),
            )
            .await;
        let new_version = input
            .branch
            .as_deref()
            .or(input.tag.as_deref())
            .unwrap_or("latest");
        output::print_transaction_table(
            &[output::TransactionRow::single_version(
                format!("{}/{}", resolved_provider, input.name),
                new_version,
                disk_impact.net,
                disk_impact.download,
            )],
            &disk_impact,
            "Net disk change:",
        );
        output::confirm_yes_default_or_cancel("Proceed with installation?")?;

        let worker = BuildWorker::new(self.provider_manager);
        let build_result = worker
            .build(
                BuildRequest {
                    name: input.name.clone(),
                    repo_slug: resolved_repo_slug.clone(),
                    provider: resolved_provider.clone(),
                    base_url: resolved_base_url.clone(),
                    version_tag: input.tag,
                    branch: input.branch,
                    requested_profile: input.build_profile,
                    build_output: input.build_output.map(std::path::PathBuf::from),
                },
                input.channel.clone(),
            )
            .await?;

        println!(
            "{}",
            output::title(format!(
                "Built artifact: {} ({:?})",
                build_result.artifact_path.display(),
                build_result.profile
            ))
        );

        let mut package = Package::with_defaults(
            input.name,
            resolved_repo_slug,
            Filetype::Binary,
            None,
            None,
            input.channel,
            resolved_provider,
            resolved_base_url,
        );
        package.install_type = InstallType::Build;
        package.build_branch = build_result.branch.clone();
        package.build_commit = build_result.commit.clone();

        let mut install_operation = InstallOperation::new(
            self.provider_manager,
            self.package_storage,
            self.paths,
            self.trusted_keys.clone(),
        )?;
        let mut msg = Some(|_: &str| {});
        let installed = install_operation
            .install_local_artifact(
                package,
                &build_result.artifact_path,
                build_result.version,
                &input.desktop,
                &mut msg,
            )
            .await?;

        println!(
            "{}",
            output::success(format!("Build install complete for '{}'.", installed.name))
        );

        Ok(())
    }

    async fn estimate_build_disk_impact(
        &self,
        input: &BuildCommandInput,
        repo_slug: &str,
        provider: &Provider,
        base_url: Option<&str>,
    ) -> DiskImpact {
        if input.branch.is_some() {
            return DiskImpact::unknown();
        }

        let release = if let Some(tag) = input.tag.as_deref() {
            self.provider_manager
                .get_release_by_tag(repo_slug, tag, provider, base_url)
                .await
        } else {
            self.provider_manager
                .get_latest_release(repo_slug, provider, &input.channel, base_url)
                .await
        };

        let Ok(release) = release else {
            return DiskImpact::unknown();
        };

        let source_size = release
            .assets
            .iter()
            .find(|asset| asset.name.starts_with("source."))
            .map(|asset| asset.size)
            .unwrap_or(0);

        install_impact_from_download(asset_size_estimate(source_size))
    }
}
