use std::path::Path;

use anyhow::{Result, anyhow};

use crate::{
    application::cancellation,
    models::{
        common::{enums::TrustMode, version::Version},
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{
            InstallRequest, InstallSource, PackageInstaller, PackagePhase, PackageProgressEvent,
            ResolvedAssetInstall,
        },
        trust::TrustedSignatureKeys,
    },
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};

pub struct ReleaseInstallRequest {
    pub package: Package,
    pub version: Option<String>,
    pub add_entry: bool,
    pub trust_mode: TrustMode,
}

pub struct PlannedReleaseInstallRequest {
    pub package: Package,
    pub plan: ResolvedAssetInstall,
    pub add_entry: bool,
    pub trust_mode: TrustMode,
}

pub struct SelectedAssetInstallRequest<'a> {
    pub package: Package,
    pub release: &'a Release,
    pub asset: &'a Asset,
    pub add_entry: bool,
    pub trust_mode: TrustMode,
}

pub struct LocalArtifactInstallRequest<'a> {
    pub package: Package,
    pub artifact_path: &'a Path,
    pub version: Version,
    pub add_entry: bool,
}

pub struct InstallOperation<'a> {
    installer: PackageInstaller<'a>,
    package_database: &'a mut PackageDatabase,
    trusted_keys: TrustedSignatureKeys,
}

impl<'a> InstallOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_database: &'a mut PackageDatabase,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
    ) -> Result<Self> {
        Ok(Self {
            installer: PackageInstaller::new(provider_manager, paths)?,
            package_database,
            trusted_keys,
        })
    }

    pub async fn preview_release_install(
        &self,
        package: &Package,
        version: &Option<String>,
        semver: &Option<String>,
    ) -> Result<ResolvedAssetInstall> {
        let plan = self
            .installer
            .plan_install(InstallRequest {
                package: package.clone(),
                source: InstallSource::Release {
                    version: version.clone(),
                    semver: semver.clone(),
                },
                add_entry: false,
                trust_mode: TrustMode::BestEffort,
            })
            .await?;
        plan.resolved_asset()
            .ok_or_else(|| anyhow!("Release install plan did not resolve an asset"))
    }

    pub async fn install_release_plan<F, H, P>(
        &mut self,
        request: PlannedReleaseInstallRequest,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        cancellation::check()?;
        self.installer
            .ensure_name_available(self.package_database, &request.package.name)?;
        let plan = self
            .installer
            .plan_install(InstallRequest {
                package: request.package,
                source: InstallSource::SelectedRelease {
                    release: request.plan.release,
                    asset: request.plan.asset,
                },
                add_entry: request.add_entry,
                trust_mode: request.trust_mode,
            })
            .await?;
        self.installer
            .install(
                self.package_database,
                &self.trusted_keys,
                plan,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
    }

    pub async fn install_release<F, H, P>(
        &mut self,
        request: ReleaseInstallRequest,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        cancellation::check()?;
        self.installer
            .ensure_name_available(self.package_database, &request.package.name)?;
        if let Some(callback) = progress_callback.as_mut() {
            callback(PackageProgressEvent::Phase(PackagePhase::ResolvingRelease));
        }
        let plan = self
            .installer
            .plan_install(InstallRequest {
                package: request.package,
                source: InstallSource::Release {
                    version: request.version,
                    semver: None,
                },
                add_entry: request.add_entry,
                trust_mode: request.trust_mode,
            })
            .await?;
        self.installer
            .install(
                self.package_database,
                &self.trusted_keys,
                plan,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
    }

    pub async fn install_selected_asset<F, H, P>(
        &mut self,
        request: SelectedAssetInstallRequest<'_>,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        cancellation::check()?;
        self.installer
            .ensure_name_available(self.package_database, &request.package.name)?;
        let plan = self
            .installer
            .plan_install(InstallRequest {
                package: request.package,
                source: InstallSource::SelectedRelease {
                    release: request.release.clone(),
                    asset: request.asset.clone(),
                },
                add_entry: request.add_entry,
                trust_mode: request.trust_mode,
            })
            .await?;
        self.installer
            .install(
                self.package_database,
                &self.trusted_keys,
                plan,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
    }

    pub async fn install_local_artifact<H, P>(
        &mut self,
        request: LocalArtifactInstallRequest<'_>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        cancellation::check()?;
        self.installer
            .ensure_name_available(self.package_database, &request.package.name)?;
        let plan = self
            .installer
            .plan_install(InstallRequest {
                package: request.package,
                source: InstallSource::LocalArtifact {
                    artifact_path: request.artifact_path.to_path_buf(),
                    version: request.version,
                },
                add_entry: request.add_entry,
                trust_mode: TrustMode::BestEffort,
            })
            .await?;
        let mut no_download_progress: Option<fn(u64, u64)> = None;
        self.installer
            .install(
                self.package_database,
                &self.trusted_keys,
                plan,
                &mut no_download_progress,
                message_callback,
                progress_callback,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::InstallOperation;
    use crate::{
        models::{
            common::enums::{Channel, Filetype, Provider},
            upstream::Package,
        },
        providers::provider_manager::ProviderManager,
        services::trust::TrustedSignatureKeys,
        storage::database::PackageDatabase,
        utils::test_support,
    };
    use std::fs;

    fn test_package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[tokio::test]
    async fn install_local_artifact_rejects_existing_package_name_before_installing() {
        let root = test_support::temp_root("upstream-install-op", "duplicate-name");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp dir");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata dir");

        let provider_manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");
        let mut package_database =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open database");
        package_database
            .upsert_package(&test_package("tool"))
            .expect("store package");

        let trusted_keys = TrustedSignatureKeys::default();
        let mut operation = InstallOperation::new(
            &provider_manager,
            &mut package_database,
            &paths,
            trusted_keys,
        )
        .expect("install operation");

        let mut message = Some(|_: &str| {});
        let mut progress: Option<fn(crate::services::packaging::PackageProgressEvent)> = None;
        let err = operation
            .install_local_artifact(
                crate::application::operations::install_op::LocalArtifactInstallRequest {
                    package: test_package("tool"),
                    artifact_path: &root.join("missing-artifact"),
                    version: crate::models::common::Version::new(1, 2, 3, false),
                    add_entry: false,
                },
                &mut message,
                &mut progress,
            )
            .await
            .expect_err("duplicate name should be rejected");

        assert!(err.to_string().contains("already exists"));
        let _ = fs::remove_dir_all(&root);
    }
}
