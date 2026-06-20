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
        packaging::{
            InstallPreview, PackageInstaller, PackagePhase, PackageProgressEvent,
            transaction_recorder::{PackageTransaction, failed_package, successful_package},
        },
        storage::{
            package_storage::PackageStorage,
            transaction_storage::{TransactionKind, UndoActionKind},
        },
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

#[derive(Debug, Clone)]
pub enum PackageTransactionContext {
    Record {
        kind: TransactionKind,
        undo_kind: Option<UndoActionKind>,
    },
    CoveredByParent,
}

impl PackageTransactionContext {
    pub fn install() -> Self {
        Self::Record {
            kind: TransactionKind::Install,
            undo_kind: Some(UndoActionKind::Remove),
        }
    }

    pub fn build() -> Self {
        Self::Record {
            kind: TransactionKind::Build,
            undo_kind: Some(UndoActionKind::Remove),
        }
    }
}

pub struct ReleaseInstallRequest {
    pub package: Package,
    pub version: Option<String>,
    pub add_entry: bool,
    pub trust_mode: TrustMode,
    pub transaction_context: PackageTransactionContext,
}

pub struct SelectedAssetInstallRequest<'a> {
    pub package: Package,
    pub release: &'a Release,
    pub asset: &'a Asset,
    pub add_entry: bool,
    pub trust_mode: TrustMode,
    pub transaction_context: PackageTransactionContext,
}

pub struct LocalArtifactInstallRequest<'a> {
    pub package: Package,
    pub artifact_path: &'a Path,
    pub version: Version,
    pub add_entry: bool,
    pub transaction_context: PackageTransactionContext,
}

pub struct InstallOperation<'a> {
    installer: PackageInstaller<'a>,
    package_storage: &'a mut PackageStorage,
    trusted_keys: TrustedSignatureKeys,
    paths: &'a UpstreamPaths,
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
            paths,
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
        let package_name = request.package.name.clone();
        let transaction =
            self.start_transaction(request.transaction_context, package_name.clone())?;

        let result = match self
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
        };

        self.finish_transaction(transaction, &package_name, result)
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
        let package_name = request.package.name.clone();
        let transaction =
            self.start_transaction(request.transaction_context, package_name.clone())?;

        let result = match self
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
        };

        self.finish_transaction(transaction, &package_name, result)
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
        let package_name = request.package.name.clone();
        let transaction =
            self.start_transaction(request.transaction_context, package_name.clone())?;

        let result = match self
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
        };

        self.finish_transaction(transaction, &package_name, result)
    }

    fn start_transaction(
        &self,
        context: PackageTransactionContext,
        package_name: String,
    ) -> Result<Option<PackageTransaction>> {
        match context {
            PackageTransactionContext::Record { kind, undo_kind } => Ok(Some(
                PackageTransaction::start(self.paths, kind, vec![package_name], undo_kind)?,
            )),
            PackageTransactionContext::CoveredByParent => Ok(None),
        }
    }

    fn finish_transaction(
        &self,
        transaction: Option<PackageTransaction>,
        package_name: &str,
        result: Result<Package>,
    ) -> Result<Package> {
        match (result, transaction) {
            (Ok(installed_package), Some(transaction)) => {
                transaction.complete(vec![successful_package(
                    package_name.to_string(),
                    None,
                    Some(installed_package.version.to_string()),
                )])?;
                Ok(installed_package)
            }
            (Err(err), Some(transaction)) => {
                let summary = crate::output::error_summary(&err);
                transaction.fail(
                    vec![failed_package(
                        package_name.to_string(),
                        None,
                        None,
                        summary.clone(),
                    )],
                    summary,
                )?;
                Err(err)
            }
            (Ok(installed_package), None) => Ok(installed_package),
            (Err(err), None) => Err(err),
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
