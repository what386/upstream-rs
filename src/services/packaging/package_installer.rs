use crate::{
    models::{common::enums::Filetype, provider::Release, upstream::Package},
    services::{
        filesystem::{ShellManager, SymlinkManager, compression_handler, permission_handler}, packaging::ChecksumVerifier, providers::provider_manager::ProviderManager
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
    paths: &'a UpstreamPaths,
    download_cache: PathBuf,
    extract_cache: PathBuf,
}

impl<'a> PackageInstaller<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        paths: &'a UpstreamPaths,
    ) -> Result<Self> {
        let temp_path = std::env::temp_dir().join(format!("upstream-{}", std::process::id()));
        let download_cache = temp_path.join("downloads");
        let extract_cache = temp_path.join("extracts");

        fs::create_dir_all(&download_cache).context(format!(
            "Failed to create download cache directory at '{}'",
            download_cache.display()
        ))?;
        fs::create_dir_all(&extract_cache).context(format!(
            "Failed to create extraction cache directory at '{}'",
            extract_cache.display()
        ))?;

        Ok(Self {
            provider_manager,
            paths,
            download_cache,
            extract_cache,
        })
    }

    /// Install package files from a release
    /// Returns the updated package with installation paths set
    pub async fn install_package_files<F, H>(
        &self,
        mut package: Package,
        release: &Release,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        message!(
            message_callback,
            "Selecting asset from '{}'",
            release.name
        );

        let best_asset = self
            .provider_manager
            .find_recommended_asset(release, &package)
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
                release,
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

        package.version = release.version.clone();

        match package.filetype {
            Filetype::AppImage => self
                .handle_appimage(&download_path, package, message_callback)
                .context("Failed to install AppImage"),
            Filetype::Compressed => self
                .handle_compressed(&download_path, package, message_callback)
                .context("Failed to install compressed file"),
            Filetype::Archive => self
                .handle_archive(&download_path, package, message_callback)
                .context("Failed to install archive"),
            _ => self
                .handle_file(&download_path, package, message_callback)
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

        permission_handler::make_executable(&exec_path).context(format!(
            "Failed to make '{}' executable",
            exec_path.display()
        ))?;

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

        message!(
            message_callback,
            "Added '{}' to PATH",
            path_to_add.display()
        );

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
        message!(message_callback, "Extracting file '{}' ...", filename);

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
                fs::copy(asset_path, &out_path).context(format!(
                    "Failed to copy AppImage to '{}'",
                    out_path.display()
                ))?;
                fs::remove_file(asset_path).context(format!(
                    "Failed to remove temporary file '{}'",
                    asset_path.display()
                ))
            })
            .context(format!(
                "Failed to move AppImage to '{}'",
                out_path.display()
            ))?;

        permission_handler::make_executable(&out_path).context(format!(
            "Failed to make AppImage '{}' executable",
            filename.to_string_lossy()
        ))?;

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
                fs::remove_file(asset_path).context(format!(
                    "Failed to remove temporary file '{}'",
                    asset_path.display()
                ))
            })
            .context(format!("Failed to move binary to '{}'", out_path.display()))?;

        permission_handler::make_executable(&out_path).context(format!(
            "Failed to make binary '{}' executable",
            filename.to_string_lossy()
        ))?;

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
