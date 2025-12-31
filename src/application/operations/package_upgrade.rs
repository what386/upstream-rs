use console::style;
use crate::{
    application::operations::verify_checksum::ChecksumVerifier,
    models::{common::enums::Filetype, upstream::Package},
    services::{
        filesystem::{
            DesktopManager, IconManager, ShellManager, SymlinkManager, compression_handler,
            permission_handler,
        },
        providers::provider_manager::ProviderManager,
        storage::package_storage::PackageStorage,
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use chrono::Utc;
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
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
    download_cache: PathBuf,
    extract_cache: PathBuf,
}

impl<'a> PackageUpgrader<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
    ) -> Result<Self> {
        let temp_path = std::env::temp_dir().join(format!("upstream-{}", std::process::id()));
        let download_cache = temp_path.join("downloads");
        let extract_cache = temp_path.join("extracts");

        fs::create_dir_all(&download_cache)
            .context(format!("Failed to create download cache directory at '{}'", download_cache.display()))?;
        fs::create_dir_all(&extract_cache)
            .context(format!("Failed to create extraction cache directory at '{}'", extract_cache.display()))?;

        Ok(Self {
            provider_manager,
            package_storage,
            paths,
            download_cache,
            extract_cache,
        })
    }

    pub async fn upgrade_all<F, G, H>(
        &mut self,
        force_option: &bool,
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let package_names: Vec<String> = self
            .package_storage
            .get_all_packages()
            .iter()
            .map(|p| p.name.clone())
            .collect();

        let total = package_names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;

        for name in package_names {
            message!(message_callback, "Checking '{}' ...", name);

            let package = self
                .package_storage
                .get_mut_package_by_name(&name)
                .ok_or_else(|| anyhow!("Package '{}' not found in storage", name))?;

            match Self::perform_upgrade(
                package,
                self.provider_manager,
                self.paths,
                force_option,
                &self.download_cache,
                &self.extract_cache,
                download_progress_callback,
                message_callback,
            )
            .await
            .context(format!("Failed to upgrade package '{}'", name))
            {
                Ok(true) => {
                    message!(
                        message_callback,
                        "{}",
                        style(format!("Package '{}' upgraded", name)).green()
                    );
                    completed += 1;
                }
                Ok(false) => {
                    message!(message_callback, "Package '{}' is already up to date", name);
                    completed += 1;
                }
                Err(e) => {
                    message!(
                        message_callback,
                        "{} {}",
                        style(format!("Upgrade failed for '{}':", name)).red(),
                        e
                    );
                    failures += 1;
                }
            }

            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        self.package_storage
            .save_packages()
            .context("Failed to save updated package information to storage")?;

        if failures > 0 {
            message!(
                message_callback,
                "{} package(s) failed to upgrade",
                failures
            );
        }

        Ok(())
    }

    pub async fn upgrade_bulk<F, G, H>(
        &mut self,
        names: &Vec<String>,
        force_option: &bool,
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let total = names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;
        let mut upgraded = 0;

        for name in names {
            message!(message_callback, "Checking '{}' ...", name);

            let package = self
                .package_storage
                .get_mut_package_by_name(name)
                .ok_or_else(|| anyhow!("Package '{}' is not installed", name))?;

            match Self::perform_upgrade(
                package,
                self.provider_manager,
                self.paths,
                force_option,
                &self.download_cache,
                &self.extract_cache,
                download_progress_callback,
                message_callback,
            )
            .await
            .context(format!("Failed to upgrade package '{}'", name))
            {
                Ok(true) => {
                    message!(
                        message_callback,
                        "{}",
                        style(format!("Package '{}' upgraded", name)).green()
                    );
                    upgraded += 1;
                }
                Ok(false) => {
                    message!(message_callback, "Package '{}' is already up to date", name);
                }
                Err(e) => {
                    message!(
                        message_callback,
                        "{} {}",
                        style(format!("Upgrade failed for '{}':", name)).red(),
                        e
                    );
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        self.package_storage
            .save_packages()
            .context("Failed to save updated package information to storage")?;

        message!(
            message_callback,
            "Completed: {} upgraded, {} up-to-date, {} failed",
            upgraded,
            total - upgraded - failures,
            failures
        );

        Ok(())
    }

    pub async fn upgrade_single<F, H>(
        &mut self,
        package_name: &str,
        force_option: &bool,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<bool>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let package = self
            .package_storage
            .get_mut_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

        let was_upgraded = Self::perform_upgrade(
            package,
            self.provider_manager,
            self.paths,
            force_option,
            &self.download_cache,
            &self.extract_cache,
            download_progress_callback,
            message_callback,
        )
        .await
        .context(format!("Failed to upgrade package '{}'", package_name))?;

        if was_upgraded {
            self.package_storage
                .save_packages()
                .context(format!("Failed to save updated information for '{}'", package_name))?;
        }

        Ok(was_upgraded)
    }

    /// Check for available updates without applying them
    /// Returns a vector of tuples: (package_name, current_version, latest_version)
    pub async fn check_updates<H>(
        &self,
        message_callback: &mut Option<H>,
    ) -> Result<Vec<(String, String, String)>>
    where
        H: FnMut(&str),
    {
        let packages = self.package_storage.get_all_packages();
        let mut updates = Vec::new();

        for package in packages {
            message!(message_callback, "Checking '{}' ...", package.name);

            match self
                .provider_manager
                .get_latest_release(&package.repo_slug, &package.provider)
                .await
                .context(format!("Failed to fetch latest release for '{}'", package.name))
            {
                Ok(latest_release) => {
                    if latest_release.version.is_newer_than(&package.version) {
                        message!(
                            message_callback,
                            "{} {} → {}",
                            style(format!("Update available for '{}':", package.name)).green(),
                            package.version,
                            latest_release.version
                        );
                        updates.push((
                            package.name.clone(),
                            package.version.to_string(),
                            latest_release.version.to_string(),
                        ));
                    } else {
                        message!(message_callback, "'{}' is up to date", package.name);
                    }
                }
                Err(e) => {
                    message!(
                        message_callback,
                        "{} {}",
                        style(format!("Failed to check '{}':", package.name)).red(),
                        e
                    );
                }
            }
        }

        Ok(updates)
    }

    /// Check if a specific package has an update available
    /// Returns Some((current_version, latest_version)) if update is available, None otherwise
    pub async fn check_single_update<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<Option<(String, String)>>
    where
        H: FnMut(&str),
    {
        let package = self
            .package_storage
            .get_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

        message!(message_callback, "Checking '{}' ...", package.name);

        let latest_release = self
            .provider_manager
            .get_latest_release(&package.repo_slug, &package.provider)
            .await
            .context(format!("Failed to fetch latest release for '{}'", package_name))?;

        if latest_release.version.is_newer_than(&package.version) {
            message!(
                message_callback,
                "{} {} → {}",
                style("Update available:").green(),
                package.version,
                latest_release.version
            );
            Ok(Some((
                package.version.to_string(),
                latest_release.version.to_string(),
            )))
        } else {
            message!(message_callback, "'{}' is up to date", package.name);
            Ok(None)
        }
    }

    async fn perform_upgrade<F, H>(
        package: &mut Package,
        provider_manager: &ProviderManager,
        paths: &UpstreamPaths,
        force_option: &bool,
        download_cache: &Path,
        extract_cache: &Path,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<bool>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        message!(message_callback, "Fetching latest release ...");

        let latest_release = provider_manager
            .get_latest_release(&package.repo_slug, &package.provider)
            .await
            .context(format!("Failed to fetch latest release for '{}'", package.repo_slug))?;

        if !*force_option && !latest_release.version.is_newer_than(&package.version) {
            message!(
                message_callback,
                "Nothing to do - '{}' is up to date",
                package.name
            );
            return Ok(false);
        }

        package.version = latest_release.version.clone();

        message!(
            message_callback,
            "Selecting asset from '{}'",
            latest_release.name
        );

        let best_asset = provider_manager
            .find_recommended_asset(&latest_release, package)
            .context(format!(
                "Could not find a compatible asset for '{}' (filetype: {:?})",
                package.name, package.filetype
            ))?;

        message!(message_callback, "Downloading '{}' ...", best_asset.name);

        let download_path = provider_manager
            .download_asset(
                &best_asset,
                &package.provider,
                download_cache,
                download_progress_callback,
            )
            .await
            .context(format!("Failed to download asset '{}'", best_asset.name))?;

        let checksum_verifier = ChecksumVerifier::new(provider_manager, download_cache);
        let verified = checksum_verifier
            .try_verify_file(
                &download_path,
                &latest_release,
                &package.provider,
                download_progress_callback,
            )
            .await
            .context("Failed to verify checksum")?;

        if verified {
            message!(message_callback, "{}", style("Checksum verified").green());
        } else {
            message!(
                message_callback,
                "{}",
                style("No checksum available, skipping verification").yellow()
            );
        }

        message!(message_callback, "Upgrading package ...");

        // Store whether we had desktop integration before
        let had_desktop_integration = package.icon_path.is_some();

        // Remove old installation before installing new version
        Self::remove_old_installation(package, paths, message_callback)
            .context(format!("Failed to remove old installation of '{}'", package.name))?;

        match package.filetype {
            Filetype::AppImage => Self::handle_appimage(
                &download_path,
                package,
                paths,
                extract_cache,
                message_callback,
            )
            .context("Failed to upgrade AppImage")?,
            Filetype::Compressed => Self::handle_compressed(
                &download_path,
                package,
                paths,
                extract_cache,
                message_callback,
            )
            .context("Failed to upgrade compressed file")?,
            Filetype::Archive => Self::handle_archive(
                &download_path,
                package,
                paths,
                extract_cache,
                message_callback,
            )
            .context("Failed to upgrade archive")?,
            _ => Self::handle_file(&download_path, package, paths, message_callback)
                .context("Failed to upgrade file")?,
        }

        // Update desktop integration if it existed before
        if had_desktop_integration {
            message!(message_callback, "Updating desktop integration ...");

            let icon_manager = IconManager::new(paths)
                .context("Failed to initialize icon manager")?;
            let desktop_manager = DesktopManager::new(paths)
                .context("Failed to initialize desktop manager")?;

            let icon_path = icon_manager
                .add_icon(
                    &package.name,
                    package.install_path.as_ref().unwrap(),
                    &package.filetype,
                )
                .await
                .context(format!("Failed to update icon for '{}'", package.name))?;

            package.icon_path = Some(icon_path);

            let _ = desktop_manager
                .create_desktop_entry(
                    &package.name,
                    package.exec_path.as_ref().unwrap(),
                    package.icon_path.as_ref().unwrap(),
                    None,
                    None,
                )
                .context(format!("Failed to update desktop entry for '{}'", package.name))?;

            message!(message_callback, "Desktop integration updated");
        }

        Ok(true)
    }

    /// Remove the old installation of a package
    fn remove_old_installation<H>(
        package: &Package,
        paths: &UpstreamPaths,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = match &package.install_path {
            Some(path) => path,
            None => return Ok(()), // Nothing to remove if not installed
        };

        message!(
            message_callback,
            "Removing old installation at '{}' ...",
            install_path.display()
        );

        // Remove from PATH if it's an archive
        if package.filetype == Filetype::Archive {
            ShellManager::new(&paths.config.paths_file, &paths.integration.symlinks_dir)
                .remove_from_paths(install_path)
                .context(format!(
                    "Failed to remove '{}' from PATH configuration",
                    install_path.display()
                ))?;
        }

        // Remove symlink
        SymlinkManager::new(&paths.integration.symlinks_dir)
            .remove_link(&package.name)
            .context(format!("Failed to remove symlink for '{}'", package.name))?;

        // Remove the actual installation
        if install_path.is_dir() {
            fs::remove_dir_all(install_path)
                .context(format!(
                    "Failed to remove old installation directory at '{}'",
                    install_path.display()
                ))?;
        } else if install_path.is_file() {
            fs::remove_file(install_path)
                .context(format!(
                    "Failed to remove old installation file at '{}'",
                    install_path.display()
                ))?;
        }

        // Remove old desktop integration if it exists
        if package.icon_path.is_some() {
            let desktop_manager = DesktopManager::new(paths)
                .context("Failed to initialize desktop manager")?;

            // Ignore errors when removing desktop entry - it might not exist
            let _ = desktop_manager.remove_entry(&package.name);

            if let Some(icon_path) = &package.icon_path {
                if icon_path.exists() {
                    fs::remove_file(icon_path)
                        .context(format!(
                            "Failed to remove old icon file at '{}'",
                            icon_path.display()
                        ))?;
                }
            }
        }

        message!(message_callback, "Old installation removed");

        Ok(())
    }

    fn handle_archive<H>(
        asset_path: &Path,
        package: &mut Package,
        paths: &UpstreamPaths,
        extract_cache: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let filename = asset_path.file_name().unwrap().display();
        message!(message_callback, "Extracting directory '{filename}' ...");

        let extracted_path = compression_handler::decompress(asset_path, extract_cache)
            .context(format!("Failed to extract archive '{}'", filename))?;

        // Fallback to handle_file if extraction resulted in a single file
        if extracted_path.is_file() {
            return Self::handle_file(&extracted_path, package, paths, message_callback);
        }

        let dirname = extracted_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid extracted path: no filename"))?;
        let out_path = paths.install.archives_dir.join(dirname);

        message!(
            message_callback,
            "Moving directory to '{}' ...",
            out_path.display()
        );

        fs::rename(&extracted_path, &out_path)
            .context(format!(
                "Failed to move extracted directory from '{}' to '{}'",
                extracted_path.display(),
                out_path.display()
            ))?;

        ShellManager::new(&paths.config.paths_file, &paths.integration.symlinks_dir)
            .add_to_paths(&out_path)
            .context(format!("Failed to add '{}' to PATH", out_path.display()))?;

        message!(message_callback, "Added '{}' to PATH", out_path.display());
        message!(message_callback, "Searching for executable ...");

        package.exec_path = if let Some(exec_path) =
            permission_handler::find_executable(&out_path, &package.name)
        {
            permission_handler::make_executable(&exec_path)
                .context(format!("Failed to make '{}' executable", exec_path.display()))?;
            message!(
                message_callback,
                "Added executable permission for '{}'",
                exec_path.file_name().unwrap().display()
            );
            Some(exec_path)
        } else {
            message!(
                message_callback,
                "{}",
                style("Could not automatically locate executable").yellow()
            );
            None
        };

        package.install_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(())
    }

    fn handle_compressed<H>(
        asset_path: &Path,
        package: &mut Package,
        paths: &UpstreamPaths,
        extract_cache: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let filename = asset_path.file_name().unwrap().display();
        message!(
            message_callback,
            "Extracting file '{}' ...",
            filename
        );

        let extracted_path = compression_handler::decompress(asset_path, extract_cache)
            .context(format!("Failed to decompress '{}'", filename))?;

        Self::handle_file(&extracted_path, package, paths, message_callback)
    }

    fn handle_appimage<H>(
        asset_path: &Path,
        package: &mut Package,
        paths: &UpstreamPaths,
        _extract_cache: &Path,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid asset path: no filename"))?;
        let out_path = paths.install.appimages_dir.join(filename);

        message!(
            message_callback,
            "Moving file to '{}' ...",
            out_path.display()
        );

        fs::rename(asset_path, &out_path)
            .or_else(|_| {
                fs::copy(asset_path, &out_path)
                    .context(format!("Failed to copy AppImage to '{}'", out_path.display()))?;
                fs::remove_file(asset_path)
                    .context(format!("Failed to remove temporary file '{}'", asset_path.display()))
            })
            .context(format!("Failed to move AppImage to '{}'", out_path.display()))?;

        permission_handler::make_executable(&out_path)
            .context(format!("Failed to make AppImage '{}' executable", filename.to_string_lossy()))?;

        message!(message_callback, "Made '{}' executable", filename.display());

        SymlinkManager::new(&paths.integration.symlinks_dir)
            .add_link(&out_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(())
    }

    fn handle_file<H>(
        asset_path: &Path,
        package: &mut Package,
        paths: &UpstreamPaths,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid asset path: no filename"))?;
        let out_path = paths.install.binaries_dir.join(filename);

        message!(
            message_callback,
            "Moving file to '{}' ...",
            out_path.display()
        );

        fs::rename(asset_path, &out_path)
            .or_else(|_| {
                fs::copy(asset_path, &out_path)
                    .context(format!("Failed to copy binary to '{}'", out_path.display()))?;
                fs::remove_file(asset_path)
                    .context(format!("Failed to remove temporary file '{}'", asset_path.display()))
            })
            .context(format!("Failed to move binary to '{}'", out_path.display()))?;

        permission_handler::make_executable(&out_path)
            .context(format!("Failed to make binary '{}' executable", filename.to_string_lossy()))?;

        message!(message_callback, "Made '{}' executable", filename.display());

        SymlinkManager::new(&paths.integration.symlinks_dir)
            .add_link(&out_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(())
    }
}

impl<'a> Drop for PackageUpgrader<'a> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.extract_cache);
        let _ = fs::remove_dir_all(&self.download_cache);
    }
}
