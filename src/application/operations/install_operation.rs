#[cfg(target_os = "linux")]
use crate::services::integration::AppImageExtractor;
use crate::{
    models::common::{DesktopEntry, enums::Filetype, enums::TrustMode},
    models::upstream::Package,
    providers::provider_manager::ProviderManager,
    services::{
        integration::{DesktopManager, IconManager},
        packaging::PackageRemover,
        storage::package_storage::PackageStorage,
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

use crate::{
    services::packaging::disk_impact::{
        DiskImpact, asset_size_estimate, install_impact_from_download,
    },
    services::packaging::{PackageInstaller, PackagePhase, PackageProgressEvent},
};

use anyhow::{Context, Result, anyhow};
use console::style;
use std::time::{Duration, Instant};

const INSTALL_PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

macro_rules! progress {
    ($cb:expr, $event:expr) => {{
        if let Some(cb) = $cb.as_mut() {
            cb($event);
        }
    }};
}

pub struct InstallOperation<'a> {
    installer: PackageInstaller<'a>,
    package_storage: &'a mut PackageStorage,
    provider_manager: &'a ProviderManager,
    paths: &'a UpstreamPaths,
    trusted_keys: TrustedSignatureKeys,
}

pub struct InstallPreview {
    pub release_name: String,
    pub release_tag: String,
    pub asset_name: String,
    pub resolved_filetype: Filetype,
    pub disk_impact: DiskImpact,
}

impl<'a> InstallOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
    ) -> Result<Self> {
        let installer = PackageInstaller::new(provider_manager, paths)?;
        Ok(Self {
            installer,
            package_storage,
            provider_manager,
            paths,
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
        let package_name = package.name.clone();

        let mut installed_package = self
            .perform_install_with_progress(
                package,
                version,
                trust_mode,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await
            .context(format!(
                "Failed to perform installation for '{}'",
                package_name
            ))?;

        if *add_entry {
            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::CreatingDesktopEntry)
            );

            if let Err(err) = self
                .add_desktop_entry(&mut installed_package, message_callback)
                .await
            {
                return self
                    .fail_after_partial_install(
                        installed_package,
                        err.context("Failed to create desktop integration"),
                        message_callback,
                    )
                    .map(|_| ());
            }
        }

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::SavingMetadata)
        );
        if let Err(err) = self
            .package_storage
            .add_or_update_package(installed_package.clone())
            .context(format!(
                "Failed to save package '{}' to storage",
                installed_package.name
            ))
        {
            return self
                .fail_after_partial_install(installed_package, err, message_callback)
                .map(|_| ());
        }

        Ok(())
    }

    pub async fn install_local_artifact<H>(
        &mut self,
        package: Package,
        artifact_path: &std::path::Path,
        version: crate::models::common::version::Version,
        add_entry: &bool,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let mut installed_package = self
            .installer
            .install_local_artifact(package, artifact_path, version, message_callback)
            .context("Failed to install local artifact")?;

        if *add_entry
            && let Err(err) = self
                .add_desktop_entry(&mut installed_package, message_callback)
                .await
        {
            return self.fail_after_partial_install(
                installed_package,
                err.context("Failed to create desktop integration"),
                message_callback,
            );
        }

        if let Err(err) = self
            .package_storage
            .add_or_update_package(installed_package.clone())
            .context(format!(
                "Failed to save package '{}' to storage",
                installed_package.name
            ))
        {
            return self.fail_after_partial_install(installed_package, err, message_callback);
        }

        Ok(installed_package)
    }

    async fn add_desktop_entry<H>(
        &self,
        installed_package: &mut Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        #[cfg(target_os = "linux")]
        let appimage_extractor =
            AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

        #[cfg(target_os = "linux")]
        let icon_manager = IconManager::new(self.paths, &appimage_extractor);
        #[cfg(not(target_os = "linux"))]
        let icon_manager = IconManager::new(self.paths);

        #[cfg(target_os = "linux")]
        let desktop_manager = DesktopManager::new(self.paths, &appimage_extractor);
        #[cfg(not(target_os = "linux"))]
        let desktop_manager = DesktopManager::new(self.paths);

        let install_path = installed_package.install_path.clone().ok_or_else(|| {
            anyhow!(
                "Package '{}' has no install path after installation",
                installed_package.name
            )
        })?;

        let icon_path = icon_manager
            .add_icon(
                &installed_package.name,
                &install_path,
                &installed_package.filetype,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to add icon for '{}'",
                installed_package.name
            ))?;

        installed_package.icon_path = icon_path;

        let desktop_entry = DesktopEntry::from_package(installed_package);

        desktop_manager
            .create_entry(
                &install_path,
                &installed_package.filetype,
                desktop_entry,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to create desktop entry for '{}'",
                installed_package.name
            ))?;

        Ok(())
    }

    fn fail_after_partial_install<H>(
        &self,
        installed_package: Package,
        err: anyhow::Error,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        match self.cleanup_partial_install(&installed_package, message_callback) {
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

    fn cleanup_partial_install<H>(
        &self,
        installed_package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        if installed_package.install_path.is_none() {
            return Ok(());
        }

        PackageRemover::new(self.paths)
            .remove_package_files(installed_package, message_callback)
            .context(format!(
                "Failed to clean up partial install for '{}'",
                installed_package.name
            ))
    }

    pub async fn preview_single_install(
        &self,
        package: &Package,
        version: &Option<String>,
    ) -> Result<InstallPreview> {
        if package.install_path.is_some() {
            return Err(anyhow!("Package '{}' is already installed", package.name));
        }

        let release = if let Some(version_tag) = version {
            self.provider_manager
                .get_release_by_tag(
                    &package.repo_slug,
                    version_tag,
                    &package.provider,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'. Verify the version tag exists",
                    version_tag, package.repo_slug
                ))?
        } else {
            self.provider_manager
                .get_latest_release(
                    &package.repo_slug,
                    &package.provider,
                    &package.channel,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch latest {} release for '{}'",
                    package.channel, package.repo_slug
                ))?
        };

        let best_asset = self
            .provider_manager
            .find_recommended_asset(&release, package)
            .context(format!(
                "Could not find a compatible asset for '{}' (filetype: {:?}, arch: detected automatically)",
                package.name, package.filetype
            ))?;

        let resolved_filetype = if package.filetype == Filetype::Auto {
            best_asset.filetype
        } else {
            package.filetype
        };

        Ok(InstallPreview {
            release_name: release.name,
            release_tag: release.tag,
            asset_name: best_asset.name.clone(),
            resolved_filetype,
            disk_impact: install_impact_from_download(asset_size_estimate(best_asset.size)),
        })
    }

    async fn perform_install_with_progress<F, H, P>(
        &self,
        package: Package,
        version: &Option<String>,
        trust_mode: TrustMode,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        if package.install_path.is_some() {
            return Err(anyhow!("Package '{}' is already installed", package.name));
        }

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::ResolvingRelease)
        );

        let release = if let Some(version_tag) = version {
            // SPECIFIC VERSION
            message!(
                message_callback,
                "Fetching release for version '{}' ...",
                version_tag
            );
            self.provider_manager
                .get_release_by_tag(
                    &package.repo_slug,
                    version_tag,
                    &package.provider,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'. Verify the version tag exists",
                    version_tag, package.repo_slug
                ))?
        } else {
            // LATEST VERSION
            message!(message_callback, "Fetching latest release ...");
            self.provider_manager
                .get_latest_release(
                    &package.repo_slug,
                    &package.provider,
                    &package.channel,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch latest {} release for '{}'",
                    package.channel, package.repo_slug
                ))?
        };

        let progress_callback = std::cell::RefCell::new(progress_callback.as_mut());
        let mut bridged_progress = Some(|event: PackageProgressEvent| {
            if let Some(cb) = progress_callback.borrow_mut().as_deref_mut() {
                cb(event);
            }
        });
        let mut bridged_download_progress = Some(|downloaded: u64, total: u64| {
            if let Some(cb) = download_progress_callback.as_mut() {
                cb(downloaded, total);
            }
            if let Some(cb) = progress_callback.borrow_mut().as_deref_mut() {
                cb(PackageProgressEvent::Download { downloaded, total });
            }
        });

        self.installer
            .install_package_files(
                package,
                &release,
                trust_mode,
                &self.trusted_keys,
                &mut bridged_download_progress,
                message_callback,
                &mut bridged_progress,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::InstallOperation;
    use crate::models::common::enums::{Channel, Filetype, Provider, TrustMode};
    use crate::models::upstream::Package;
    use crate::providers::provider_manager::ProviderManager;
    use crate::services::packaging::PackageProgressEvent;
    use crate::services::storage::package_storage::PackageStorage;
    use crate::utils::test_support;
    use std::path::Path;
    use std::{fs, io};

    fn temp_root(name: &str) -> std::path::PathBuf {
        test_support::temp_root("upstream-install-op-test", name)
    }

    fn test_paths(root: &Path) -> crate::utils::static_paths::UpstreamPaths {
        test_support::upstream_paths(root)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[tokio::test]
    async fn perform_install_rejects_already_installed_package_before_network_calls() {
        let root = temp_root("already-installed");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let provider_manager = ProviderManager::new(None, None, None).expect("provider manager");
        let op = InstallOperation::new(
            &provider_manager,
            &mut storage,
            &paths,
            crate::services::trust::TrustedSignatureKeys::default(),
        )
        .expect("operation");

        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(paths.install.binaries_dir.join("tool"));
        let mut dl: Option<fn(u64, u64)> = None;
        let mut msg = Some(|_: &str| {});
        let mut progress: Option<fn(PackageProgressEvent)> = None;

        let err = op
            .perform_install_with_progress(
                package,
                &None,
                TrustMode::BestEffort,
                &mut dl,
                &mut msg,
                &mut progress,
            )
            .await
            .expect_err("already-installed guard");
        assert!(err.to_string().contains("already installed"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn cleanup_partial_install_removes_installed_binary() {
        let root = temp_root("cleanup-partial-install");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(&paths.integration.symlinks_dir).expect("create symlinks dir");
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let provider_manager = ProviderManager::new(None, None, None).expect("provider manager");
        let op = InstallOperation::new(
            &provider_manager,
            &mut storage,
            &paths,
            crate::services::trust::TrustedSignatureKeys::default(),
        )
        .expect("operation");

        let install_path = paths.install.binaries_dir.join("tool");
        fs::write(&install_path, b"new").expect("write installed binary");
        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(install_path.clone());
        package.exec_path = Some(install_path.clone());
        let mut msg = Some(|_: &str| {});

        op.cleanup_partial_install(&package, &mut msg)
            .expect("cleanup partial install");

        assert!(!install_path.exists());
        cleanup(&root).expect("cleanup");
    }
}
