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
use std::{
    fs,
    path::{Path, PathBuf},
};

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
    fn backup_path(install_path: &Path) -> Result<PathBuf> {
        let file_name = install_path.file_name().ok_or_else(|| {
            anyhow::anyhow!("Install path '{}' has no filename", install_path.display())
        })?;
        Ok(install_path.with_file_name(format!("{}.old", file_name.to_string_lossy())))
    }

    fn remove_path_if_exists(path: &Path) -> Result<()> {
        if path.is_dir() {
            fs::remove_dir_all(path)
                .context(format!("Failed to remove directory '{}'", path.display()))?;
        } else if path.is_file() {
            fs::remove_file(path).context(format!("Failed to remove file '{}'", path.display()))?;
        }
        Ok(())
    }

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
    pub async fn upgrade<F, H>(
        &self,
        package: &Package,
        force: bool,
        ignore_checksums: bool,
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

        let latest_release = if force {
            self.provider_manager
                .get_latest_release(&package.repo_slug, &package.provider, &package.channel)
                .await
                .context(format!(
                    "Failed to fetch latest release for '{}'",
                    package.name
                ))?
        } else {
            let Some(latest_release) = self
                .provider_manager
                .check_for_updates(package)
                .await
                .context(format!(
                    "Failed to fetch latest release for '{}'",
                    package.name
                ))?
            else {
                message!(message_callback, "'{}' is already up to date", package.name);
                return Ok(None);
            };

            latest_release
        };

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

        let original_install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!("Package '{}' has no install path recorded", package.name)
            })?
            .clone();
        let backup_path = Self::backup_path(&original_install_path)?;

        Self::remove_path_if_exists(&backup_path)?;

        message!(
            message_callback,
            "Backing up existing install to '{}' ...",
            backup_path.display()
        );
        fs::rename(&original_install_path, &backup_path).context(format!(
            "Failed to back up '{}' to '{}'",
            original_install_path.display(),
            backup_path.display()
        ))?;

        // Remove runtime integrations (PATH/symlink) but keep desktop assets
        if let Err(e) = self
            .remover
            .remove_runtime_integrations(package, message_callback)
        {
            let _ = fs::rename(&backup_path, &original_install_path);
            let _ = self
                .remover
                .restore_runtime_integrations(package, message_callback);
            return Err(e).context(format!(
                "Failed to remove runtime integration for '{}'",
                package.name
            ));
        }

        // Install new version
        let install_result = self
            .installer
            .install_package_files(
                package.clone(),
                &latest_release,
                ignore_checksums,
                download_progress,
                message_callback,
            )
            .await;
        let mut updated_package = match install_result {
            Ok(updated_package) => updated_package,
            Err(install_err) => {
                message!(
                    message_callback,
                    "{}",
                    style(format!(
                        "Upgrade failed for '{}', rolling back ...",
                        package.name
                    ))
                    .yellow()
                );

                let _ = Self::remove_path_if_exists(&original_install_path);
                fs::rename(&backup_path, &original_install_path).context(format!(
                    "Upgrade failed for '{}': {}. Rollback failed while restoring backup",
                    package.name, install_err
                ))?;

                self.remover
                    .restore_runtime_integrations(package, message_callback)
                    .context(format!(
                        "Upgrade failed for '{}': {}. Rollback failed while restoring runtime links",
                        package.name, install_err
                    ))?;

                return Err(install_err).context(format!(
                    "Failed to install new version of '{}' (previous version restored)",
                    package.name
                ));
            }
        };

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
                .create_entry(
                    &updated_package.name,
                    updated_package.install_path.as_ref().unwrap(),
                    updated_package.exec_path.as_ref().unwrap(),
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

        Self::remove_path_if_exists(&backup_path)
            .context(format!("Failed to remove backup for '{}'", package.name))?;

        Ok(Some(updated_package))
    }
}

#[cfg(test)]
#[path = "../../../tests/services/packaging/package_upgrader.rs"]
mod tests;
