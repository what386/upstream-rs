#[cfg(target_os = "linux")]
use crate::services::integration::AppImageExtractor;
use crate::{
    models::common::{DesktopEntry, enums::Filetype, enums::TrustMode},
    models::upstream::Package,
    providers::provider_manager::ProviderManager,
    services::{
        integration::{DesktopManager, IconManager},
        storage::package_storage::PackageStorage,
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

use crate::services::packaging::PackageInstaller;

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
        let package_name = package.name.clone();

        let mut installed_package = self
            .perform_install(
                package,
                version,
                trust_mode,
                download_progress_callback,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to perform installation for '{}'",
                package_name
            ))?;

        if *add_entry {
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

            let desktop_entry = DesktopEntry::from_package(&installed_package);

            let _ = desktop_manager
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
        }

        self.package_storage
            .add_or_update_package(installed_package.clone())
            .context(format!(
                "Failed to save package '{}' to storage",
                installed_package.name
            ))?;

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

        if *add_entry {
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

            let desktop_entry = DesktopEntry::from_package(&installed_package);

            let _ = desktop_manager
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
        }

        self.package_storage
            .add_or_update_package(installed_package.clone())
            .context(format!(
                "Failed to save package '{}' to storage",
                installed_package.name
            ))?;

        Ok(installed_package)
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
        })
    }

    async fn perform_install<F, H>(
        &self,
        package: Package,
        version: &Option<String>,
        trust_mode: TrustMode,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        if package.install_path.is_some() {
            return Err(anyhow!("Package '{}' is already installed", package.name));
        }

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

        self.installer
            .install_package_files(
                package,
                &release,
                trust_mode,
                &self.trusted_keys,
                download_progress_callback,
                message_callback,
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
    use crate::services::storage::package_storage::PackageStorage;
    use crate::utils::static_paths::{
        AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths,
    };
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-install-op-test-{name}-{nanos}"))
    }

    fn test_paths(root: &Path) -> UpstreamPaths {
        let dirs = AppDirs {
            user_dir: root.to_path_buf(),
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            metadata_dir: root.join("data/metadata"),
        };

        UpstreamPaths {
            config: ConfigPaths {
                config_file: dirs.config_dir.join("config.toml"),
                packages_file: dirs.metadata_dir.join("packages.json"),
                metadata_file: dirs.metadata_dir.join("metadata.json"),
                paths_file: dirs.metadata_dir.join("paths.sh"),
            },
            install: InstallPaths {
                appimages_dir: dirs.data_dir.join("appimages"),
                binaries_dir: dirs.data_dir.join("binaries"),
                archives_dir: dirs.data_dir.join("archives"),
                rollback_dir: dirs.data_dir.join("rollback"),
            },
            integration: IntegrationPaths {
                symlinks_dir: dirs.data_dir.join("symlinks"),
                xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
                icons_dir: dirs.data_dir.join("icons"),
                bash_completions_dir: dirs
                    .user_dir
                    .join(".local/share/bash-completion/completions"),
                fish_completions_dir: dirs.user_dir.join(".config/fish/completions"),
                zsh_completions_dir: dirs.user_dir.join(".local/share/zsh/site-functions"),
            },
            dirs,
        }
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
        let mut msg: Option<fn(&str)> = None;

        let err = op
            .perform_install(package, &None, TrustMode::BestEffort, &mut dl, &mut msg)
            .await
            .expect_err("already-installed guard");
        assert!(err.to_string().contains("already installed"));

        cleanup(&root).expect("cleanup");
    }
}
