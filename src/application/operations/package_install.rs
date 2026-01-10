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
            .perform_install(package, version, download_progress_callback, message_callback)
            .await
            .context(format!("Failed to perform installation for '{}'", package_name))?;

        if *add_entry {
            let icon_manager = IconManager::new(self.paths)
                .context("Failed to initialize icon manager")?;
            let desktop_manager = DesktopManager::new(self.paths)
                .context("Failed to initialize desktop manager")?;

            let icon_path = icon_manager
                .add_icon(
                    &installed_package.name,
                    installed_package.install_path.as_ref().unwrap(),
                    &installed_package.filetype,
                )
                .await
                .context(format!("Failed to add icon for '{}'", installed_package.name))?;

            installed_package.icon_path = Some(icon_path);

            let _ = desktop_manager.create_desktop_entry(
                &installed_package.name,
                installed_package.exec_path.as_ref().unwrap(),
                installed_package.icon_path.as_ref().unwrap(),
                None,
                None,
            )
            .context(format!("Failed to create desktop entry for '{}'", installed_package.name))?;
        }

        self.package_storage
            .add_or_update_package(installed_package.clone())
            .context(format!("Failed to save package '{}' to storage", installed_package.name))?;

        Ok(())
    }

    async fn perform_install<F, H>(
        &self,
        mut package: Package,
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

        let latest_release = if let Some(version_tag) = version {
            message!(message_callback, "Fetching release for version '{}' ...", version_tag);
            self.provider_manager
                .get_release_by_tag(&package.repo_slug, version_tag, &package.provider)
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'. Verify the version tag exists",
                    version_tag, package.repo_slug
                ))?
        } else {
            message!(message_callback, "Fetching latest release ...");
            self.provider_manager
                .get_latest_release(&package.repo_slug, &package.provider)
                .await
                .context(format!("Failed to fetch latest release for '{}'", package.repo_slug))?
        };

        package.version = latest_release.version.clone();

        message!(
            message_callback,
            "Selecting asset from '{}'",
            latest_release.name
        );

        let best_asset = self
            .provider_manager
            .find_recommended_asset(&latest_release, &package)
            .context(format!(
                "Could not find a compatible asset for '{}' (filetype: {:?}, arch: detected automatically)",
                package.name, package.filetype
            ))?;

        message!(message_callback, "Downloading '{}' ...", best_asset.name);

        let download_path = self
            .provider_manager
            .download_asset(
                &best_asset,
                &package.provider,
                &self.download_cache,
                download_progress_callback,
            )
            .await
            .context(format!("Failed to download asset '{}'", best_asset.name))?;

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
            Filetype::AppImage => self.handle_appimage(&download_path, package, message_callback)
                .context("Failed to install AppImage"),
            Filetype::Compressed => self.handle_compressed(&download_path, package, message_callback)
                .context("Failed to install compressed file"),
            Filetype::Archive => self.handle_archive(&download_path, package, message_callback)
                .context("Failed to install archive"),
            _ => self.handle_file(&download_path, package, message_callback)
                .context("Failed to install file"),
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

        let extracted_path = compression_handler::decompress(asset_path, &self.extract_cache)
            .context(format!("Failed to extract archive '{}'", filename))?;

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

        fs::rename(&extracted_path, &out_path).context(format!(
            "Failed to move extracted directory from '{}' to '{}'",
            extracted_path.display(),
            out_path.display()
        ))?;

        let shell_manager = ShellManager::new(
            &self.paths.config.paths_file,
            &self.paths.integration.symlinks_dir,
        );

        message!(message_callback, "Searching for executable ...");

        let Some(exec_path) = permission_handler::find_executable(&out_path, &package.name) else {
            message!(
                message_callback,
                "{}",
                style("Could not automatically locate executable").yellow()
            );

            // Fallback: add out_path to PATH
            shell_manager
                .add_to_paths(&out_path)
                .context(format!("Failed to add '{}' to PATH", out_path.display()))?;
            message!(message_callback, "Added '{}' to PATH", out_path.display());

            package.exec_path = None;
            package.install_path = Some(out_path);
            package.last_upgraded = Utc::now();
            return Ok(package);
        };

        permission_handler::make_executable(&exec_path)
            .context(format!("Failed to make '{}' executable", exec_path.display()))?;

        message!(
            message_callback,
            "Added executable permission for '{}'",
            exec_path.file_name().unwrap().display()
        );

        let path_to_add = exec_path
            .parent()
            .ok_or_else(|| anyhow!("Executable has no parent directory"))?;


        shell_manager
            .add_to_paths(path_to_add)
            .context(format!("Failed to add '{}' to PATH", path_to_add.display()))?;
        message!(message_callback, "Added '{}' to PATH", path_to_add.display());

        package.exec_path = Some(exec_path);
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
        let filename = asset_path.file_name().unwrap().display();
        message!(
            message_callback,
            "Extracting file '{}' ...",
            filename
        );

        let extracted_path = compression_handler::decompress(asset_path, &self.extract_cache)
            .context(format!("Failed to decompress '{}'", filename))?;

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

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
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

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
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
        Ok(package)
    }
}

impl<'a> Drop for PackageInstaller<'a> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.extract_cache);
        let _ = fs::remove_dir_all(&self.download_cache);
    }
}
