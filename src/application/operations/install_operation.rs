use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::{
    models::{
        common::{enums::TrustMode, version::Version},
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{InstallPreview, PackageInstaller, PackagePhase, PackageProgressEvent},
        storage::package_storage::PackageStorage,
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

pub struct ReleaseInstallRequest {
    pub package: Package,
    pub version: Option<String>,
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
    package_storage: &'a mut PackageStorage,
    trusted_keys: TrustedSignatureKeys,
}

impl<'a> InstallOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
    ) -> Result<Self> {
        Ok(Self {
            installer: PackageInstaller::new(provider_manager, paths)?,
            package_storage,
            trusted_keys,
        })
    }

    pub async fn preview_release_install(
        &self,
        package: &Package,
        version: &Option<String>,
    ) -> Result<InstallPreview> {
        self.installer
            .preview_single_install(package, version)
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
            .package_storage
            .add_or_update_package(installed_package.clone())
            .context(format!(
                "Failed to save package '{}' to storage",
                installed_package.name
            ))
        {
            return self.fail_after_metadata_error(installed_package, err, message_callback);
        }

        Ok(installed_package)
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
