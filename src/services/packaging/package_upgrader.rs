use crate::{
    models::{common::enums::Channel, upstream::Package},
    providers::provider_manager::ProviderManager,
    services::{
        integration::{AppImageExtractor, DesktopManager, IconManager},
        packaging::{PackageInstaller, PackageRemover},
    },
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result};
use console::style;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct PackageUpgrader<'a> {
    provider_manager: &'a ProviderManager,
    installer: PackageInstaller<'a>,
    remover: PackageRemover<'a>,
    paths: &'a UpstreamPaths,
}

impl<'a> PackageUpgrader<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        installer: PackageInstaller<'a>,
        remover: PackageRemover<'a>,
        paths: &'a UpstreamPaths,
    ) -> Self {
        Self {
            provider_manager,
            installer,
            remover,
            paths,
        }
    }

    /// Upgrade a single package.
    ///
    /// Returns:
    /// - Ok(None) => no upgrade needed
    /// - Ok(Some(Package)) => upgraded package
    pub async fn upgrade_quiet(&self, package: &Package, force: bool) -> Result<Option<Package>> {
        let mut no_download_progress: Option<fn(u64, u64)> = None;
        let mut no_messages: Option<fn(&str)> = None;
        self.upgrade(package, force, &mut no_download_progress, &mut no_messages)
            .await
    }

    /// Upgrade a single package.
    ///
    /// Returns:
    /// - Ok(None) => no upgrade needed
    /// - Ok(Some(Package)) => upgraded package
    pub async fn upgrade<F, H>(
        &self,
        package: &Package,
        force: bool,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<Option<Package>>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        if package.is_pinned {
            message!(
                message_callback,
                "Upgrade skipped: '{}' is pinned",
                package.name
            );
            return Ok(None);
        }

        message!(message_callback, "Fetching latest release ...");

        let latest_release = self
            .provider_manager
            .get_latest_release(&package.repo_slug, &package.provider, &package.channel)
            .await
            .context(format!(
                "Failed to fetch latest release for '{}'",
                package.name
            ))?;

        if !force {
            let up_to_date = if package.channel == Channel::Nightly {
                latest_release.published_at <= package.last_upgraded
            } else {
                !latest_release.version.is_newer_than(&package.version)
            };

            if up_to_date {
                message!(message_callback, "'{}' is already up to date", package.name);
                return Ok(None);
            }
        }

        let had_desktop_integration = package.icon_path.is_some();

        message!(
            message_callback,
            "{}",
            style(format!("Upgrading '{}' ...", package.name)).cyan()
        );

        // Remove old installation
        self.remover
            .remove_package_files(package, message_callback)
            .context(format!(
                "Failed to remove old installation of '{}'",
                package.name
            ))?;

        // Install new version
        let mut updated_package = self
            .installer
            .install_package_files(
                package.clone(),
                &latest_release,
                download_progress,
                message_callback,
            )
            .await
            .context(format!(
                "Failed to install new version of '{}'",
                package.name
            ))?;

        // Restore desktop integration if it existed before
        if had_desktop_integration {
            message!(message_callback, "Restoring desktop integration ...");

            let appimage_extractor =
                AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

            let icon_manager = IconManager::new(self.paths, &appimage_extractor);
            let desktop_manager = DesktopManager::new(self.paths, &appimage_extractor);

            let icon_path = icon_manager
                .add_icon(
                    &updated_package.name,
                    updated_package.install_path.as_ref().unwrap(),
                    &updated_package.filetype,
                    message_callback,
                )
                .await
                .context(format!("Failed to add icon for '{}'", updated_package.name))?;

            updated_package.icon_path = icon_path;

            let _ = desktop_manager
                .create_desktop_entry(
                    &updated_package.name,
                    &updated_package.install_path.as_ref().unwrap(),
                    &updated_package.exec_path.as_ref().unwrap(),
                    updated_package.icon_path.as_deref(),
                    &updated_package.filetype,
                    None,
                    None,
                    message_callback,
                )
                .await
                .context(format!(
                    "Failed to create desktop entry for '{}'",
                    updated_package.name
                ))?;
        }

        Ok(Some(updated_package))
    }
}
