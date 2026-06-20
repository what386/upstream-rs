use anyhow::{Context, Result};
use console::style;
use std::time::{Duration, Instant};

use crate::{
    models::common::enums::TrustMode,
    models::provider::{Asset, Release},
    models::upstream::Package,
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{
            InstallPreview, PackageInstaller, PackageProgressEvent, PackageTransactionContext,
        },
        storage::package_storage::PackageStorage,
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

const INSTALL_PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
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

    pub async fn install_bulk<F, G, H>(
        &mut self,
        packages: Vec<Package>,
        trust_mode: TrustMode,
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let total = packages.len() as u32;
        let mut completed = 0;
        let mut failures = 0;

        for package in packages {
            let package_name = package.name.clone();
            message!(message_callback, "Installing '{}' ...", package_name);

            let use_icon = &package.icon_path.is_some();
            let mut last_progress: Option<(u64, u64)> = None;
            let mut last_emit: Option<Instant> = None;
            let mut throttled_download_progress = download_progress_callback.as_mut().map(|cb| {
                |downloaded: u64, total: u64| {
                    last_progress = Some((downloaded, total));
                    let should_emit = last_emit
                        .map(|t| t.elapsed() >= INSTALL_PROGRESS_UPDATE_INTERVAL)
                        .unwrap_or(true);
                    if should_emit {
                        cb(downloaded, total);
                        last_emit = Some(Instant::now());
                    }
                }
            });

            match self
                .install_single(
                    package,
                    &None,
                    use_icon,
                    trust_mode,
                    &mut throttled_download_progress,
                    message_callback,
                )
                .await
                .context(format!("Failed to install package '{}'", package_name))
            {
                Ok(_) => {
                    message!(message_callback, "{}", style("Package installed").green());
                }
                Err(e) => {
                    message!(message_callback, "{} {}", style("Install failed:").red(), e);
                    failures += 1;
                }
            }

            if let (Some((downloaded, total)), Some(cb)) =
                (last_progress, download_progress_callback.as_mut())
            {
                cb(downloaded, total);
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        if failures > 0 {
            message!(
                message_callback,
                "{} package(s) failed to install",
                failures
            );
        }

        Ok(())
    }

    pub async fn install_single<F, H>(
        &mut self,
        package: Package,
        version: &Option<String>,
        add_entry: &bool,
        trust_mode: TrustMode,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let mut no_progress: Option<fn(PackageProgressEvent)> = None;
        self.install_single_with_progress(
            package,
            version,
            add_entry,
            trust_mode,
            download_progress_callback,
            message_callback,
            &mut no_progress,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn install_single_with_context<F, H>(
        &mut self,
        package: Package,
        version: &Option<String>,
        add_entry: &bool,
        trust_mode: TrustMode,
        transaction_context: PackageTransactionContext,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let mut no_progress: Option<fn(PackageProgressEvent)> = None;
        self.installer
            .install_release_with_progress(
                self.package_storage,
                &self.trusted_keys,
                package,
                version,
                add_entry,
                trust_mode,
                transaction_context,
                download_progress_callback,
                message_callback,
                &mut no_progress,
            )
            .await
            .map(|_| ())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn install_single_with_progress<F, H, P>(
        &mut self,
        package: Package,
        version: &Option<String>,
        add_entry: &bool,
        trust_mode: TrustMode,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        self.installer
            .install_release_with_progress(
                self.package_storage,
                &self.trusted_keys,
                package,
                version,
                add_entry,
                trust_mode,
                PackageTransactionContext::install(),
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
            .map(|_| ())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn install_selected_asset_with_progress<F, H, P>(
        &mut self,
        package: Package,
        release: &Release,
        asset: &Asset,
        add_entry: &bool,
        trust_mode: TrustMode,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        self.installer
            .install_selected_asset_with_progress(
                self.package_storage,
                &self.trusted_keys,
                package,
                release,
                asset,
                add_entry,
                trust_mode,
                PackageTransactionContext::install(),
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
            .map(|_| ())
    }

    pub async fn install_local_artifact<H>(
        &mut self,
        package: Package,
        artifact_path: &std::path::Path,
        version: crate::models::common::version::Version,
        add_entry: &bool,
        transaction_context: PackageTransactionContext,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        self.installer
            .install_local_artifact(
                self.package_storage,
                package,
                artifact_path,
                version,
                add_entry,
                transaction_context,
                message_callback,
            )
            .await
    }

    pub async fn preview_single_install(
        &self,
        package: &Package,
        version: &Option<String>,
    ) -> Result<InstallPreview> {
        self.installer
            .preview_single_install(package, version)
            .await
    }
}
