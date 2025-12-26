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

            let use_icon = &package.icon_path.is_some();

            match self
                .install_single(
                    package,
                    use_icon,
                    download_progress_callback,
                    message_callback,
                )
                .await
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
        add_entry: &bool,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let mut installed_package = self
            .perform_install(package, download_progress_callback, message_callback)
            .await?;

        if *add_entry {
            let icon_manager = IconManager::new(self.paths)?;
            let desktop_manager = DesktopManager::new(self.paths)?;
            let icon_path = icon_manager
                .add_icon(
                    &installed_package.name,
                    installed_package.install_path.as_ref().unwrap(),
                    &installed_package.filetype,
                )
                .await?;
            installed_package.icon_path = Some(icon_path);
            let _ = desktop_manager.create_desktop_entry(
                &installed_package.name,
                installed_package.exec_path.as_ref().unwrap(),
                installed_package.icon_path.as_ref().unwrap(),
                None,
                None,
            )?;
        }

        self.package_storage
            .add_or_update_package(installed_package)?;

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

        let latest_release = self
            .provider_manager
            .get_latest_release(&package.repo_slug, &package.provider)
            .await?;

        package.version = latest_release.version.clone();

        message!(
            message_callback,
            "Selecting asset from '{}'",
            latest_release.name
        );

        let best_asset = self
            .provider_manager
            .find_recommended_asset(&latest_release, &package)?;

        message!(message_callback, "Downloading '{}' ...", best_asset.name);

        let download_path = self
            .provider_manager
            .download_asset(
                &best_asset,
                &package.provider,
                &self.download_cache,
                download_progress_callback,
            )
            .await?;

        message!(message_callback, "Verifying checksum ...");

        let checksum_verifier = ChecksumVerifier::new(self.provider_manager, &self.download_cache);

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

        message!(message_callback, "Installing package ...");

        match package.filetype {
            Filetype::AppImage => self.handle_appimage(&download_path, package, message_callback),
            Filetype::Compressed => {
                self.handle_compressed(&download_path, package, message_callback)
            }
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

        message!(message_callback, "Extracting directory '{filename}' ...");

        let extracted_path = compression_handler::decompress(asset_path, &self.extract_cache)?;

        if extracted_path.is_file() {
            return self.handle_file(&extracted_path, package, message_callback);
        }

        let dirname = extracted_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = self.paths.install.archives_dir.join(dirname);

        message!(
            message_callback,
            "Moving directory to '{}' ...",
            out_path.display()
        );

        fs::rename(extracted_path, &out_path)?;

        ShellManager::new(
            &self.paths.config.paths_file,
            &self.paths.integration.symlinks_dir,
        )
        .add_to_paths(&out_path)?;

        message!(message_callback, "Added '{}' to PATH", out_path.display());
        message!(message_callback, "Searching for executable ...");

        package.exec_path = if let Some(exec_path) =
            permission_handler::find_executable(&out_path, &package.name)
        {
            permission_handler::make_executable(&exec_path)?;
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
        message!(
            message_callback,
            "Extracting file '{}' ...",
            asset_path.file_name().unwrap().display()
        );

        let extracted_path = compression_handler::decompress(asset_path, &self.extract_cache)?;

        self.handle_file(&extracted_path, package, message_callback)
    }
    fn handle_appimage<H>(
        &self,
        asset_path: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

        let out_path = self.paths.install.appimages_dir.join(filename);

        message!(
            message_callback,
            "Moving file to '{}' ...",
            out_path.display()
        );

        fs::rename(asset_path, &out_path).or_else(|_| {
            fs::copy(asset_path, &out_path)?;
            fs::remove_file(asset_path)
        })?;

        permission_handler::make_executable(&out_path)?;

        message!(message_callback, "Made '{}' executable", filename.display());

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .add_link(&out_path, &package.name)?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();

        Ok(package)
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
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

        let out_path = self.paths.install.binaries_dir.join(filename);

        message!(
            message_callback,
            "Moving file to '{}' ...",
            out_path.display()
        );

        fs::rename(asset_path, &out_path).or_else(|_| {
            fs::copy(asset_path, &out_path)?;
            fs::remove_file(asset_path)
        })?;

        permission_handler::make_executable(&out_path)?;

        message!(message_callback, "Made '{}' executable", filename.display());

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .add_link(&out_path, &package.name)?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();

        Ok(package)
    }
}

impl<'a> Drop for PackageInstaller<'a> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.extract_cache);
        let _ = fs::remove_dir_all(&self.download_cache);
    }
}
