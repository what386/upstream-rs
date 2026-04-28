use anyhow::{Result, anyhow, bail};
use console::style;

use crate::application::operations::install_operation::InstallOperation;
use crate::models::{
    common::enums::{Channel, Filetype, Provider},
    upstream::{InstallType, Package},
};
use crate::providers::discovery::{SourceKind, infer_source};
use crate::providers::provider_manager::ProviderManager;
use crate::services::builder::{BuildProfile, BuildRequest, worker::BuildWorker};
use crate::services::storage::package_storage::PackageStorage;
use crate::utils::static_paths::UpstreamPaths;

pub struct BuildOperation<'a> {
    provider_manager: &'a ProviderManager,
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
}

pub struct BuildCommandInput {
    pub name: String,
    pub repo_slug: String,
    pub tag: Option<String>,
    pub provider: Option<Provider>,
    pub base_url: Option<String>,
    pub channel: Channel,
    pub desktop: bool,
    pub build_profile: Option<BuildProfile>,
    pub build_output: Option<String>,
}

impl<'a> BuildOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
    ) -> Self {
        Self {
            provider_manager,
            package_storage,
            paths,
        }
    }

    pub async fn build_and_install(&mut self, input: BuildCommandInput) -> Result<()> {
        let (resolved_repo_slug, resolved_provider, resolved_base_url) =
            if let Some(selected) = input.provider {
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

        let worker = BuildWorker::new(self.provider_manager);
        let output = worker
            .build(
                BuildRequest {
                    name: input.name.clone(),
                    repo_slug: resolved_repo_slug.clone(),
                    provider: resolved_provider.clone(),
                    base_url: resolved_base_url.clone(),
                    version_tag: input.tag,
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

        let mut install_operation =
            InstallOperation::new(self.provider_manager, self.package_storage, self.paths)?;
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
            style(format!(
                "Build install complete for '{}'. Future 'upstream upgrade' runs will rebuild from source.",
                installed.name
            ))
            .green()
        );

        Ok(())
    }
}
