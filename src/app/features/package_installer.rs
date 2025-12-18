use std::{fs, path::{Path, PathBuf}};
use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::{
    models::{
        common::enums::Filetype,
        upstream::Package,
    },
    services::{
        providers::provider_manager::ProviderManager,
        filesystem::{file_decompressor, file_permissions, ShellIntegrator, SymlinkManager},
        storage::package_storage::PackageStorage,
    },
    utils::static_paths::UpstreamPaths,
};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct PackageInstaller<'a> {
    provider_manager: &'a ProviderManager,
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
    download_cache: PathBuf,
    extract_cache: PathBuf,
}

impl<'a> PackageInstaller<'a> {
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
            message!(message_callback, "Installing '{}' ...", package.name);

            match self.install_single(
                package,
                download_progress_callback,
                message_callback,
            ).await {
                Ok(_) => {
                    message!(message_callback, "Package installed!");
                },
                Err(e) => {
                    message!(message_callback, "Install failed: {}", e);
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        if failures > 0 {
            message!(message_callback, "{} package(s) failed to install", failures);
        }

        Ok(())
    }

    pub async fn install_single<F, H>(
        &mut self,
        package: Package,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let installed_package = self.perform_install(
            package,
            download_progress_callback,
            message_callback,
        ).await?;

        self.package_storage.add_or_update_package(installed_package)?;

        Ok(())
    }

    async fn perform_install<F, H>(
        &self,
        mut package: Package,
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

        message!(message_callback, "Fetching latest release ...");
        let latest_release = self.provider_manager
            .get_latest_release(&package.repo_slug, &package.provider)
            .await?;
        package.version = latest_release.version.clone();

        message!(message_callback, "Selecting asset from '{}'", latest_release.name);
        let best_asset = self.provider_manager.find_recommended_asset(&latest_release, &package)?;

        message!(message_callback, "Downloading '{}' ...", best_asset.name);
        let download_path = self.provider_manager
            .download_asset(&best_asset, &package.provider, &self.download_cache, download_progress_callback)
            .await?;

        message!(message_callback, "Installing package ...");
        match package.package_kind {
            Filetype::AppImage => self.handle_appimage(&download_path, package, message_callback),
            Filetype::Compressed => self.handle_compressed(&download_path, package, message_callback),
            Filetype::Archive => self.handle_archive(&download_path, package, message_callback),
            _ => self.handle_file(&download_path, package, message_callback),
        }
    }

    fn handle_archive<H>(
        &self,
        asset_path: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path.file_name().unwrap().display();
        message!(message_callback, "Extracting '{filename}' ...");

        let extracted_path = file_decompressor::decompress(asset_path, &self.extract_cache)?;
        let dirname = extracted_path.file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = self.paths.install.archives_dir.join(dirname);

        message!(message_callback, "Moving directory to '{}' ...", out_path.display());
        fs::rename(extracted_path, &out_path)?;

        ShellIntegrator::new(&self.paths.config.paths_file, &self.paths.integration.symlinks_dir).add_to_paths(&out_path)?;

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
        Ok(package)
    }

    fn handle_compressed<H>(
        &self,
        asset_path: &Path,
        package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Extracting '{}' ...", asset_path.file_name().unwrap().display());
        let extracted_path = file_decompressor::decompress(asset_path, &self.extract_cache)?;
        self.handle_file(&extracted_path, package, message_callback)
    }

    fn handle_appimage<H>(
        &self,
        asset_path: &Path,
        package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        // TODO: logic that unpacks appimage to get app icon/name
        self.handle_file(asset_path, package, message_callback)
    }

    fn handle_file<H>(
        &self,
        asset_path: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path.file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = self.paths.install.binaries_dir.join(filename);

        message!(message_callback, "Moving file to '{}' ...", out_path.display());
        fs::rename(asset_path, &out_path)
            .or_else(|_| {
                fs::copy(asset_path, &out_path)?;
                fs::remove_file(asset_path)
            })?;

        file_permissions::make_executable(&out_path)?;
        message!(message_callback, "Made '{}' executable", filename.display());

        SymlinkManager::new(&self.paths.integration.symlinks_dir).add_link(&out_path, &package.name)?;

        message!(message_callback, "Created symlink: {} → {}", package.name, out_path.display());

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }
}

impl<'a> Drop for PackageInstaller<'a> {
    fn drop(&mut self) {
        // Clean up temp directories when installer is dropped
        let temp_path = std::env::temp_dir().join("upstream");
        let _ = fs::remove_dir_all(&temp_path);
    }
}
