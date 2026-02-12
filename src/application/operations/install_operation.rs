use crate::{
    models::upstream::Package,
    providers::provider_manager::ProviderManager,
    services::{
        integration::{AppImageExtractor, DesktopManager, IconManager},
        storage::package_storage::PackageStorage,
    },
    utils::static_paths::UpstreamPaths,
};

use crate::services::packaging::PackageInstaller;

use anyhow::{Context, Result, anyhow};
use console::style;

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
}

impl<'a> InstallOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
    ) -> Result<Self> {
        let installer = PackageInstaller::new(provider_manager, paths)?;
        Ok(Self {
            installer,
            package_storage,
            provider_manager,
            paths,
        })
    }

    pub async fn install_bulk<F, G, H>(
        &mut self,
        packages: Vec<Package>,
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

            match self
                .install_single(
                    package,
                    &None,
                    use_icon,
                    download_progress_callback,
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
                download_progress_callback,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to perform installation for '{}'",
                package_name
            ))?;

        if *add_entry {
            let appimage_extractor =
                AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

            let icon_manager = IconManager::new(self.paths, &appimage_extractor);
            let desktop_manager = DesktopManager::new(self.paths, &appimage_extractor);

            let icon_path = icon_manager
                .add_icon(
                    &installed_package.name,
                    installed_package.install_path.as_ref().unwrap(),
                    &installed_package.filetype,
                    message_callback,
                )
                .await
                .context(format!(
                    "Failed to add icon for '{}'",
                    installed_package.name
                ))?;

            installed_package.icon_path = icon_path;

            let _ = desktop_manager
                .create_desktop_entry(
                    &installed_package.name,
                    &installed_package.install_path.as_ref().unwrap(),
                    &installed_package.exec_path.as_ref().unwrap(),
                    installed_package.icon_path.as_deref(),
                    &installed_package.filetype,
                    None,
                    None,
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

    async fn perform_install<F, H>(
        &self,
        package: Package,
        version: &Option<String>,
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
                .get_release_by_tag(&package.repo_slug, version_tag, &package.provider)
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'. Verify the version tag exists",
                    version_tag, package.repo_slug
                ))?
        } else {
            // LATEST VERSION
            message!(message_callback, "Fetching latest release ...");
            self.provider_manager
                .get_latest_release(&package.repo_slug, &package.provider, &package.channel)
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
                download_progress_callback,
                message_callback,
            )
            .await
    }
}
