use crate::{
    models::{common::enums::Filetype, provider::Release, upstream::Package},
    providers::provider_manager::ProviderManager,
    services::{
        integration::{ShellManager, SymlinkManager, compression_handler, permission_handler},
        packaging::ChecksumVerifier,
    },
    utils::{fs_move, static_paths::UpstreamPaths},
};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use console::style;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
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
    fn package_cache_key(package_name: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let sanitized = package_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();

        format!("{}-{}", sanitized, timestamp)
    }

    pub fn new(provider_manager: &'a ProviderManager, paths: &'a UpstreamPaths) -> Result<Self> {
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
        ignore_checksums: bool,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let cache_key = Self::package_cache_key(&package.name);
        let package_download_cache = self.download_cache.join(&cache_key);
        let package_extract_cache = self.extract_cache.join(&cache_key);
        fs::create_dir_all(&package_download_cache).context(format!(
            "Failed to create package download cache '{}'",
            package_download_cache.display()
        ))?;
        fs::create_dir_all(&package_extract_cache).context(format!(
            "Failed to create package extraction cache '{}'",
            package_extract_cache.display()
        ))?;

        message!(message_callback, "Selecting asset from '{}'", release.name);

        let best_asset = self
            .provider_manager
            .find_recommended_asset(release, &package)
            .context(format!(
                "Could not find a compatible asset for '{}' (filetype: {:?}, arch: detected automatically)",
                package.name, package.filetype
            ))?;

        if package.filetype == Filetype::Auto {
            message!(
                message_callback,
                "Resolved filetype to '{}'",
                &best_asset.filetype
            );
            package.filetype = best_asset.filetype;
        }

        message!(message_callback, "Downloading '{}' ...", best_asset.name);

        let download_path = self
            .provider_manager
            .download_asset(
                &best_asset,
                &package.provider,
                &package_download_cache,
                download_progress_callback,
            )
            .await
            .context(format!("Failed to download asset '{}'", best_asset.name))?;

        if ignore_checksums {
            message!(
                message_callback,
                "{}",
                style("Skipping checksum verification (--ignore-checksums)").yellow()
            );
        } else {
            message!(message_callback, "Verifying checksum ...");

            let checksum_verifier =
                ChecksumVerifier::new(self.provider_manager, &package_download_cache);
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
        }

        message!(message_callback, "Installing package ...");

        package.version = release.version.clone();

        match package.filetype {
            Filetype::AppImage => self
                .handle_appimage(&download_path, package, message_callback)
                .context("Failed to install AppImage"),
            Filetype::MacApp => self
                .handle_macos_app_bundle(&download_path, package, message_callback)
                .context("Failed to install macOS app bundle"),
            Filetype::Compressed => self
                .handle_compressed(
                    &download_path,
                    &package_extract_cache,
                    package,
                    message_callback,
                )
                .context("Failed to install compressed file"),
            Filetype::Archive => self
                .handle_archive(
                    &download_path,
                    &package_extract_cache,
                    package,
                    message_callback,
                )
                .context("Failed to install archive"),
            _ => self
                .handle_file(&download_path, package, message_callback)
                .context("Failed to install file"),
        }
    }

    fn handle_archive<H>(
        &self,
        asset_path: &Path,
        extract_cache: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path.file_name().unwrap().display();
        message!(message_callback, "Extracting directory '{filename}' ...");

        let extracted_path = compression_handler::decompress(asset_path, extract_cache)
            .context(format!("Failed to extract archive '{}'", filename))?;

        if extracted_path.is_file() {
            return self.handle_file(&extracted_path, package, message_callback);
        }

        if let Some(app_bundle_path) =
            Self::find_macos_app_bundle(&extracted_path, &package.name)
                .context("Failed to detect .app bundle in extracted archive")?
        {
            return self.handle_macos_app_bundle(&app_bundle_path, package, message_callback);
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

        fs_move::move_file_or_dir(&extracted_path, &out_path).context(format!(
            "Failed to move extracted directory from '{}' to '{}'",
            extracted_path.display(),
            out_path.display()
        ))?;

        let shell_manager = ShellManager::new(&self.paths.config.paths_file);

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

        let symlink_manager = SymlinkManager::new(&self.paths.integration.symlinks_dir);

        symlink_manager
            .add_link(&exec_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

        package.exec_path = Some(exec_path);
        package.install_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }

    fn is_app_bundle(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
    }

    fn find_macos_app_bundle(extracted_path: &Path, package_name: &str) -> Result<Option<PathBuf>> {
        if extracted_path.is_dir() && Self::is_app_bundle(extracted_path) {
            return Ok(Some(extracted_path.to_path_buf()));
        }

        if !extracted_path.is_dir() {
            return Ok(None);
        }

        let package_name_lower = package_name.to_lowercase();
        let mut bundles = Vec::new();

        for entry in fs::read_dir(extracted_path).context(format!(
            "Failed to read extracted directory '{}'",
            extracted_path.display()
        ))? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() && Self::is_app_bundle(&path) {
                bundles.push(path);
            }
        }

        if bundles.is_empty() {
            return Ok(None);
        }

        bundles.sort_by_key(|path| {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if stem == package_name_lower {
                0
            } else if stem.contains(&package_name_lower) {
                1
            } else {
                2
            }
        });

        Ok(bundles.into_iter().next())
    }

    fn find_macos_app_executable(app_bundle_path: &Path, package_name: &str) -> Result<PathBuf> {
        let macos_dir = app_bundle_path.join("Contents").join("MacOS");
        if !macos_dir.is_dir() {
            return Err(anyhow!(
                "Invalid .app bundle '{}': missing Contents/MacOS",
                app_bundle_path.display()
            ));
        }

        let package_name_lower = package_name.to_lowercase();
        let mut executables = Vec::new();

        for entry in fs::read_dir(&macos_dir).context(format!(
            "Failed to read app executable directory '{}'",
            macos_dir.display()
        ))? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_file() || file_type.is_symlink() {
                executables.push(entry.path());
            }
        }

        if executables.is_empty() {
            return Err(anyhow!("No executable found in '{}'", macos_dir.display()));
        }

        executables.sort_by_key(|path| {
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if file_name == package_name_lower {
                0
            } else if file_name.starts_with(&package_name_lower) {
                1
            } else {
                2
            }
        });

        Ok(executables.remove(0))
    }

    fn handle_macos_app_bundle<H>(
        &self,
        app_bundle_path: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        if !Self::is_app_bundle(app_bundle_path) {
            return Err(anyhow!(
                "Expected .app bundle path, got '{}'",
                app_bundle_path.display()
            ));
        }

        // Some providers may expose a single executable with ".app" suffix.
        // In that case, fall back to normal file installation.
        if app_bundle_path.is_file() {
            return self.handle_file(app_bundle_path, package, message_callback);
        }

        let bundle_name = app_bundle_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid .app path: no filename"))?;
        let out_path = self.paths.install.archives_dir.join(bundle_name);

        message!(
            message_callback,
            "Moving app bundle to '{}' ...",
            out_path.display()
        );

        fs_move::move_file_or_dir(app_bundle_path, &out_path).context(format!(
            "Failed to move app bundle to '{}'",
            out_path.display()
        ))?;

        let exec_path = Self::find_macos_app_executable(&out_path, &package.name)?;
        permission_handler::make_executable(&exec_path).context(format!(
            "Failed to make app executable '{}' executable",
            exec_path.display()
        ))?;

        message!(
            message_callback,
            "Using app executable '{}'",
            exec_path.display()
        );

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .add_link(&exec_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            exec_path.display()
        );

        package.install_path = Some(out_path);
        package.exec_path = Some(exec_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }

    fn handle_compressed<H>(
        &self,
        asset_path: &Path,
        extract_cache: &Path,
        package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path.file_name().unwrap().display();
        message!(message_callback, "Extracting file '{}' ...", filename);

        let extracted_path = compression_handler::decompress(asset_path, extract_cache)
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

        fs_move::move_file_or_dir(asset_path, &out_path).context(format!(
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

        fs_move::move_file_or_dir(asset_path, &out_path)
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

#[cfg(test)]
#[path = "../../../tests/services/packaging/package_installer.rs"]
mod tests;
