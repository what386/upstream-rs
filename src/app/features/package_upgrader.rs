use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::models::common::enums::Filetype;
use crate::models::upstream::Package;
use crate::services::providers::provider_manager::ProviderManager;
use crate::services::filesystem::{file_decompressor, file_permissions, ShellIntegrator, SymlinkManager};
use crate::services::storage::package_storage::PackageStorage;

use crate::utils::static_paths::UpstreamPaths;

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

        fs::create_dir_all(&download_cache)?;
        fs::create_dir_all(&extract_cache)?;

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
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let package_names: Vec<String> = self.package_storage
            .get_all_packages()
            .iter()
            .map(|p| p.name.clone())
            .collect();

        let total = package_names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;
        let mut upgraded = 0;

        for name in package_names {
            message!(message_callback, "Checking '{}' ...", name);

            let package = self.package_storage.get_mut_package_by_name(&name)
                .ok_or(anyhow!("Package '{}' not found", name))?;

            match Self::perform_install(
                package,
                self.provider_manager,
                self.paths,
                &self.download_cache,
                &self.extract_cache,
                download_progress_callback,
                message_callback,
            ).await {
                Ok(true) => {
                    message!(message_callback, "Package '{}' upgraded!", name);
                    upgraded += 1;
                },
                Ok(false) => {
                    message!(message_callback, "Package '{}' is already up to date.", name);
                },
                Err(e) => {
                    message!(message_callback, "Upgrade failed for '{}': {}", name, e);
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        self.package_storage.save_packages()?;

        if failures > 0 {
            message!(message_callback, "{} package(s) failed to upgrade", failures);
        }
        message!(message_callback, "Completed: {} upgraded, {} up-to-date, {} failed",
                 upgraded, total - upgraded - failures, failures);

        Ok(())
    }

    pub async fn install_single<F, H>(
        &mut self,
        package_name: &str,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<bool>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let package = self.package_storage.get_mut_package_by_name(package_name)
            .ok_or(anyhow!("Package '{}' is not installed.", package_name))?;

        let was_upgraded = Self::perform_install(
            package,
            self.provider_manager,
            self.paths,
            &self.download_cache,
            &self.extract_cache,
            download_progress_callback,
            message_callback,
        ).await?;

        if was_upgraded {
            self.package_storage.save_packages()?;
        }

        Ok(was_upgraded)
    }

    async fn perform_install<F, H>(
        package: &mut Package,
        provider_manager: &ProviderManager,
        paths: &UpstreamPaths,
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
            .await?;

        if !latest_release.version.is_newer_than(&package.version) {
            message!(message_callback, "Nothing to do - '{}' is up to date.", package.name);
            return Ok(false);
        }

        package.version = latest_release.version.clone();

        message!(message_callback, "Selecting asset from '{}'", latest_release.name);
        let best_asset = provider_manager.find_recommended_asset(&latest_release, package)?;

        message!(message_callback, "Downloading '{}' ...", best_asset.name);
        let download_path = provider_manager
            .download_asset(&best_asset, &package.provider, download_cache, download_progress_callback)
            .await?;

        message!(message_callback, "Upgrading package ...");
        match package.filetype {
            Filetype::AppImage => Self::handle_appimage(&download_path, package, paths, extract_cache, message_callback),
            Filetype::Compressed => Self::handle_compressed(&download_path, package, paths, extract_cache, message_callback),
            Filetype::Archive => Self::handle_archive(&download_path, package, paths, extract_cache, message_callback),
            _ => Self::handle_file(&download_path, package, paths, message_callback),
        }?;

        Ok(true)
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
        message!(message_callback, "Extracting '{filename}' ...");

        let extracted_path = file_decompressor::decompress(asset_path, extract_cache)?;
        let dirname = extracted_path.file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = paths.install.archives_dir.join(dirname);

        message!(message_callback, "Moving directory to '{}' ...", out_path.display());
        fs::rename(extracted_path, &out_path)?;

        ShellIntegrator::new(&paths.config.paths_file, &paths.integration.symlinks_dir)
            .add_to_paths(&out_path)?;

        message!(message_callback, "Added '{}' to PATH", out_path.display());

        message!(message_callback, "Searching for executable ...");
        package.exec_path = if let Some(exec_path) = file_permissions::find_executable(&out_path, &package.name) {
            file_permissions::make_executable(&exec_path)?;
            message!(message_callback, "Added executable permission for '{}'", exec_path.file_name().unwrap().display());
            Some(exec_path)
        } else {
            message!(message_callback, "Could not automatically locate executable");
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
        message!(message_callback, "Extracting '{}' ...", asset_path.file_name().unwrap().display());
        let extracted_path = file_decompressor::decompress(asset_path, extract_cache)?;
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
        // TODO: logic that unpacks appimage to get app icon/name
        Self::handle_file(asset_path, package, paths, message_callback)
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
        let filename = asset_path.file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = paths.install.binaries_dir.join(filename);

        message!(message_callback, "Moving file to '{}' ...", out_path.display());
        fs::rename(asset_path, &out_path)
            .or_else(|_| {
                fs::copy(asset_path, &out_path)?;
                fs::remove_file(asset_path)
            })?;

        file_permissions::make_executable(&out_path)?;
        message!(message_callback, "Made '{}' executable", filename.display());

        SymlinkManager::new(&paths.integration.symlinks_dir)
            .add_link(&out_path, &package.name)?;

        message!(message_callback, "Created symlink: {} → {}", package.name, out_path.display());

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(())
    }
}

impl<'a> Drop for PackageUpgrader<'a> {
    fn drop(&mut self) {
        // Clean up temp directories when installer is dropped
        let temp_path = std::env::temp_dir().join("upstream");
        let _ = fs::remove_dir_all(&temp_path);
    }
}
