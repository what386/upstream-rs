use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::{
    application::cancellation,
    models::{
        common::{enums::TrustMode, version::Version},
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    services::{
        integration::ShellManager,
        packaging::{InstallPlan, PackageInstaller, PackagePhase, PackageProgressEvent},
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
    pub plan: InstallPlan,
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
    ) -> Result<InstallPlan> {
        self.installer
            .preview_single_install(package, version)
            .await
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
        self.ensure_package_name_available(&request.package.name)?;
        let installed_package = self
            .installer
            .install_selected_asset(
                &self.trusted_keys,
                request.package,
                &request.plan.release,
                &request.plan.asset,
                &request.add_entry,
                request.trust_mode,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await?;
        self.save_installed_package(installed_package, message_callback, progress_callback)
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
        self.ensure_package_name_available(&request.package.name)?;
        match self
            .installer
            .install_release(
                &self.trusted_keys,
                request.package,
                &request.version,
                &request.add_entry,
                request.trust_mode,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
        {
            Ok(installed_package) => {
                self.save_installed_package(installed_package, message_callback, progress_callback)
            }
            Err(err) => Err(err),
        }
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
        self.ensure_package_name_available(&request.package.name)?;
        match self
            .installer
            .install_selected_asset(
                &self.trusted_keys,
                request.package,
                request.release,
                request.asset,
                &request.add_entry,
                request.trust_mode,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
        {
            Ok(installed_package) => {
                self.save_installed_package(installed_package, message_callback, progress_callback)
            }
            Err(err) => Err(err),
        }
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
        self.ensure_package_name_available(&request.package.name)?;
        match self
            .installer
            .install_local_artifact(
                request.package,
                request.artifact_path,
                request.version,
                &request.add_entry,
                message_callback,
                progress_callback,
            )
            .await
        {
            Ok(installed_package) => {
                self.save_installed_package(installed_package, message_callback, progress_callback)
            }
            Err(err) => Err(err),
        }
    }

    fn save_installed_package<H, P>(
        &mut self,
        installed_package: Package,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        if let Some(cb) = progress_callback.as_mut() {
            cb(PackageProgressEvent::Phase(PackagePhase::SavingMetadata));
        }

        if let Err(err) = self
            .package_database
            .upsert_package(&installed_package)
            .context(format!(
                "Failed to save package '{}' to storage",
                installed_package.name
            ))
        {
            return self.fail_after_metadata_error(installed_package, err, message_callback);
        }

        if let Err(err) = ShellManager::new(&self.installer.paths().config.paths_file)
            .regenerate_paths(self.package_database, self.installer.paths())
        {
            let _ = self
                .package_database
                .remove_package(&installed_package.name);
            return self.fail_after_metadata_error(installed_package, err, message_callback);
        }

        Ok(installed_package)
    }

    fn ensure_package_name_available(&self, name: &str) -> Result<()> {
        if self.package_database.package_exists(name)? {
            return Err(anyhow!("Package '{}' already exists", name));
        }

        Ok(())
    }

    fn fail_after_metadata_error<H>(
        &self,
        installed_package: Package,
        err: anyhow::Error,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        match self
            .installer
            .cleanup_partial_install(&installed_package, message_callback)
        {
            Ok(()) => Err(err.context(format!(
                "Rolled back partial install for '{}'",
                installed_package.name
            ))),
            Err(cleanup_err) => Err(anyhow!(
                "{}. Additionally failed to roll back partial install for '{}': {}",
                err,
                installed_package.name,
                cleanup_err
            )),
        }
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
            PackageDatabase::open(&paths.config.packages_database_file).expect("open database");
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
