use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::models::common::enums::Filetype;
use crate::models::upstream::Package;

use crate::services::providers::provider_manager::ProviderManager;
use crate::services::filesystem::{file_decompressor, symlink_handling, file_permissions, shell_integration};

use crate::utils::upstream_paths::PATHS;

macro_rules! emit {
    ($msg:expr, $cb:expr) => {{
        if let Some(cb) = $cb {
            cb($msg);
        }
    }};
}

pub async fn perform_upgrade(
    package: Package,
    provider_manager: &ProviderManager,
    progress: Option<&mut dyn FnMut(u64, u64)>,
    message: &mut Option<&mut dyn FnMut(String)>
) -> Result<Package> {
    emit!(format!("Fetching latest release ..."), message);
    let latest_release = provider_manager.get_latest_release(&package.repo_slug, &package.provider).await?;

    if !latest_release.version.is_newer_than(&package.version) {
        return Err(anyhow!("Nothing to do - '{}' is up to date.", package.name));
    }

    emit!(format!("Selecting asset from '{}'", latest_release.name), message);
    let best_asset = provider_manager.find_recommended_asset(&latest_release, &package)?;

    emit!(format!("Downloading '{}' ...", best_asset.name), message);
    let download_path = provider_manager.download_asset(&best_asset, &package.provider , progress).await?;

    emit!(format!("Upgrading package ..."), message);
    let installed_package = match package.pkg_kind {
        Filetype::AppImage => handle_appimage(&download_path, package, message),
        Filetype::Compressed => handle_compressed(&download_path, package, message),
        Filetype::Archive => handle_archive(&download_path, package, message),
        _ => handle_file(&download_path, package, message)
    }?;

    Ok(installed_package)
}

fn handle_archive(asset_path: &Path, mut package: Package, message: &mut Option<&mut dyn FnMut(String)>) -> Result<Package> {
    let cache_path = std::env::temp_dir().join("upstream_extraction");

    emit!(format!("Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy()), message);
    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;

    let dirname = extracted_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.archives_dir.join(dirname);

    emit!(format!("Removing old install ..."), message);
    fs::remove_dir_all(&out_path)?;

    shell_integration::remove_from_paths(&out_path)?;
    emit!(format!("Removed '{}' from PATH", package.name), message);

    emit!(format!("Moving new directory to '{}' ...", out_path.to_string_lossy()), message);
    fs::rename(extracted_path, &out_path)?;

    shell_integration::add_to_paths(&out_path)?;
    emit!(format!("Added '{}' to PATH", out_path.to_string_lossy()), message);

    emit!(format!("Searching for executable ..."), message);
    if let Some(exec_path) = file_permissions::find_executable(&out_path, &package.name) {
        let exec_name = exec_path.file_name().unwrap().to_string_lossy();
        emit!(format!("Found executable: {}", exec_name), message);

        file_permissions::make_executable(&exec_path)?;
        package.exec_path = Some(exec_path.clone());
        emit!(format!("Added executable permission for '{}'", exec_name), message);
    } else {
        emit!(format!("Could not automatically locate executable"), message);
        package.exec_path = None;
    }

    package.install_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(package)
}

fn handle_compressed(asset_path: &Path, package: Package, message: &mut Option<&mut dyn FnMut(String)>) -> Result<Package> {
    let cache_path = std::env::temp_dir().join("upstream_extraction");

    emit!(format!("Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy()), message);
    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;

    handle_file(&extracted_path, package, message)
}

fn handle_appimage(asset_path: &Path, mut package: Package, message: &mut Option<&mut dyn FnMut(String)>) -> Result<Package> {
    // TODO: logic that unpacks appimage to get app icon/name
    handle_file(asset_path, package, message)
}

fn handle_file(asset_path: &Path, mut package: Package, message: &mut Option<&mut dyn FnMut(String)>) -> Result<Package> {
    let filename = asset_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.binaries_dir.join(filename);

    emit!(format!("Removing old install ..."), message);
    fs::remove_file(&out_path)?;

    symlink_handling::remove_link(&package.name)?;
    emit!(format!("Removed symlink for '{}'", package.name ), message);

    emit!(format!("Moving new files to '{}' ...", out_path.to_string_lossy()), message);
    fs::rename(asset_path, &out_path)?;

    file_permissions::make_executable(&out_path)?;
    emit!(format!("Made '{}' executable", out_path.file_name().unwrap().to_string_lossy()), message);

    symlink_handling::add_link(&out_path, &package.name)?;
    emit!(format!("Created symlink: {} -> {}", asset_path.to_string_lossy(), out_path.to_string_lossy()), message);

    package.install_path = Some(out_path.clone());
    package.exec_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(package)
}
