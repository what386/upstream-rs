use anyhow::{Context, Result, anyhow, bail};
use console::style;

use crate::application::{operations::install_operation::InstallOperation, output};
use crate::models::{
    common::enums::{Channel, Filetype, Provider},
    upstream::{InstallType, Package},
};
use crate::providers::discovery::{SourceKind, infer_source};
use crate::providers::provider_manager::ProviderManager;
use crate::services::builder::{BuildProfile, BuildRequest, worker::BuildWorker};
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
            input.provider
        {
            (input.repo_slug.clone(), selected, input.base_url.clone())
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

        println!(
            "{}",
            style(format!(
                "Building {} from {} ...",
                &input.name, &resolved_provider
            ))
            .cyan()
        );

        if input.dry_run {
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
                println!("{}", style("Dry run: build preview").bold());
                println!("  package: {}", input.name);
                println!("  source: {} ({})", resolved_repo_slug, resolved_provider);
                println!("  ref: branch {} @ {}", branch, commit);
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
                println!("{}", style("Dry run: build preview").bold());
                println!("  package: {}", input.name);
                println!("  source: {} ({})", resolved_repo_slug, resolved_provider);
                println!("  ref: release {} ({})", release.name, release.tag);
            }

            match input.build_profile {
                Some(profile) => println!("  profile: {:?}", profile),
                None => println!("  profile: auto-detect at build time"),
            }
            if let Some(path) = input.build_output.as_deref() {
                println!("  build output override: {}", path);
            } else {
                println!("  build output override: none");
            }
            println!(
                "  desktop entry: {}",
                if input.desktop { "yes" } else { "no" }
            );
            println!("  actions: resolve only (no compile, no install, no metadata changes)");
            return Ok(());
        }

        output::confirm_or_cancel(format!(
            "Build and install '{}' from {} ({})?",
            input.name, resolved_repo_slug, resolved_provider
        ))?;

        let worker = BuildWorker::new(self.provider_manager);
        let output = worker
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
            style(format!(
                "Built artifact: {} ({:?})",
                output.artifact_path.display(),
                output.profile
            ))
            .cyan()
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
        package.build_branch = output.branch.clone();
        package.build_commit = output.commit.clone();

        let mut install_operation = InstallOperation::new(
            self.provider_manager,
            self.package_storage,
            self.paths,
            self.trusted_keys.clone(),
        )?;
        let mut msg = Some(|line: &str| println!("{line}"));
        let installed = install_operation
            .install_local_artifact(
                package,
                &output.artifact_path,
                output.version,
                &input.desktop,
                &mut msg,
            )
            .await?;

        println!(
            "{}",
            style(format!("Build install complete for '{}'.", installed.name)).green()
        );

        Ok(())
    }
}
