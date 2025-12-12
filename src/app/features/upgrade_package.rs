use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::models::common::enums::Filetype;
use crate::models::upstream::Package;

use crate::services::providers::provider_manager::{Credentials, ProviderManager};
use crate::services::filesystem::{file_decompressor, symlink_handling, file_permissions, shell_integration};
use crate::services::storage::package_storage::PackageStorage;

use crate::utils::upstream_paths::PATHS;

/*
*   callback example:
*
    let mut message_callback = |msg: &str| {
        println!("{}", msg);
        // or: log::info!("{}", msg);
    };
*/

macro_rules! emit {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb {
            cb(&format!($($arg)*));
        }
    }};
}

pub async fn upgrade(
    credentials: Credentials,
    package_name: String,
    progress: Option<&mut dyn FnMut(u64, u64)>,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    let temp_path = std::env::temp_dir().join("upstream");
    let download_cache = temp_path.join("downloads");
    let extraction_cache = temp_path.join("extracts");

    fs::create_dir_all(&download_cache)?;
    fs::create_dir_all(&extraction_cache)?;

    let provider_manager = ProviderManager::new(credentials, &download_cache)?;
    let mut package_store = PackageStorage::new()?;

    let package = package_store.get_mut_package_by_name(&package_name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", package_name))?;

    perform_upgrade(
        package,
        &provider_manager,
        &extraction_cache,
        progress,
        message
    ).await?;

    package_store.save_packages()?;

    let _ = fs::remove_dir_all(&temp_path);

    emit!(message, "Package installed!");
    Ok(())
}

pub async fn perform_upgrade(
    package: &mut Package,
    provider_manager: &ProviderManager,
    extract_cache: &Path,
    progress: Option<&mut dyn FnMut(u64, u64)>,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    emit!(message, "Fetching latest release ...");
    let latest_release = provider_manager.get_latest_release(&package.repo_slug, &package.provider).await?;

    if !latest_release.version.is_newer_than(&package.version) {
        return Err(anyhow!("Nothing to do - '{}' is up to date.", package.name));
    }

    emit!(message, "Selecting asset from '{}'", latest_release.name);
    let best_asset = provider_manager.find_recommended_asset(&latest_release, &package)?;

    emit!(message, "Downloading '{}' ...", best_asset.name);
    let download_path = provider_manager.download_asset(&best_asset, &package.provider, progress).await?;

    emit!(message, "Upgrading package ...");
    match package.filetype {
        Filetype::AppImage => handle_appimage(&download_path, package, message),
        Filetype::Compressed => handle_compressed(&download_path, extract_cache, package, message),
        Filetype::Archive => handle_archive(&download_path, extract_cache, package, message),
        _ => handle_file(&download_path, package, message)
    }?;

    Ok(())
}

fn handle_archive(
    asset_path: &Path,
    cache_path: &Path,
    package: &mut Package,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    emit!(message, "Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy());
    let extracted_path = file_decompressor::decompress(asset_path, cache_path)?;

    let dirname = extracted_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.archives_dir.join(dirname);

    emit!(message, "Removing old install ...");
    fs::remove_dir_all(&out_path)?;

    shell_integration::remove_from_paths(&out_path)?;
    emit!(message, "Removed '{}' from PATH", package.name);

    emit!(message, "Moving new directory to '{}' ...", out_path.to_string_lossy());
    fs::rename(extracted_path, &out_path)?;

    shell_integration::add_to_paths(&out_path)?;
    emit!(message, "Added '{}' to PATH", out_path.to_string_lossy());

    emit!(message, "Searching for executable ...");
    if let Some(exec_path) = file_permissions::find_executable(&out_path, &package.name) {
        let exec_name = exec_path.file_name().unwrap().to_string_lossy();
        emit!(message, "Found executable: {}", exec_name);

        file_permissions::make_executable(&exec_path)?;
        package.exec_path = Some(exec_path.clone());
        emit!(message, "Added executable permission for '{}'", exec_name);
    } else {
        emit!(message, "Could not automatically locate executable");
        package.exec_path = None;
    }

    package.install_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(())
}

fn handle_compressed(
    asset_path: &Path,
    cache_path: &Path,
    package: &mut Package,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    emit!(message, "Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy());
    let extracted_path = file_decompressor::decompress(asset_path, cache_path)?;

    handle_file(&extracted_path, package, message)
}

fn handle_appimage(
    asset_path: &Path,
    package: &mut Package,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    // TODO: logic that unpacks appimage to get app icon/name
    handle_file(asset_path, package, message)
}

fn handle_file(
    asset_path: &Path,
    package: &mut Package,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    let filename = asset_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.binaries_dir.join(filename);

    emit!(message, "Removing old install ...");
    fs::remove_file(&out_path)?;

    symlink_handling::remove_link(&package.name)?;
    emit!(message, "Removed symlink for '{}'", package.name);

    emit!(message, "Moving new files to '{}' ...", out_path.to_string_lossy());

    if let Err(_) = fs::rename(asset_path, &out_path) {
        fs::copy(asset_path, &out_path)?;
        fs::remove_file(asset_path)?;
    }

    file_permissions::make_executable(&out_path)?;
    emit!(message, "Made '{}' executable", out_path.file_name().unwrap().to_string_lossy());

    symlink_handling::add_link(&out_path, &package.name)?;
    emit!(message, "Created symlink: {} -> {}", package.name, out_path.to_string_lossy());

    package.install_path = Some(out_path.clone());
    package.exec_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(())
}
