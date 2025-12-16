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

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub async fn install_bulk<F, G, H>(
    credentials: Credentials,
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

        match install_single(
            credentials.clone(),
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
    credentials: Credentials,
    package: Package,
    download_progress_callback: &mut Option<F>,
    message_callback: &mut Option<H>,
) -> Result<()>
where
    F: FnMut(u64, u64),
    H: FnMut(&str),
{
    let temp_path = std::env::temp_dir().join("upstream");
    let download_cache = temp_path.join("downloads");
    let extraction_cache = temp_path.join("extracts");

    fs::create_dir_all(&download_cache)?;
    fs::create_dir_all(&extraction_cache)?;

    let provider_manager = ProviderManager::new(credentials)?;
    let mut package_store = PackageStorage::new()?;

    let installed_package = perform_install(
        package,
        &provider_manager,
        &download_cache,
        &extraction_cache,
        download_progress_callback,
        message_callback,
    ).await?;

    package_store.add_or_update_package(installed_package)?;
    fs::remove_dir_all(&temp_path)?;

    Ok(())
}

async fn perform_install<F, H>(
    mut package: Package,
    provider_manager: &ProviderManager,
    download_cache: &Path,
    extract_cache: &Path,
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
    let latest_release = provider_manager.get_latest_release(&package.repo_slug, &package.provider).await?;
    package.version = latest_release.version.clone();

    message!(message_callback, "Selecting asset from '{}'", latest_release.name);
    let best_asset = provider_manager.find_recommended_asset(&latest_release, &package)?;

    message!(message_callback, "Downloading '{}' ...", best_asset.name);
    let download_path = provider_manager.download_asset(&best_asset, &package.provider, download_cache, download_progress_callback).await?;

    message!(message_callback, "Installing package ...");
    let installed_package = match package.filetype {
        Filetype::AppImage => handle_appimage(&download_path, package, message_callback),
        Filetype::Compressed => handle_compressed(&download_path, extract_cache, package, message_callback),
        Filetype::Archive => handle_archive(&download_path, extract_cache, package, message_callback),
        _ => handle_file(&download_path, package, message_callback)
    }?;

    Ok(installed_package)
}

fn handle_archive<H>(
    asset_path: &Path,
    cache_path: &Path,
    mut package: Package,
    message_callback: &mut Option<H>,
) -> Result<Package>
where
    H: FnMut(&str),
{
    message!(message_callback, "Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy());
    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;

    let dirname = extracted_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.archives_dir.join(dirname);

    message!(message_callback, "Moving directory to '{}' ...", out_path.to_string_lossy());
    fs::rename(extracted_path, &out_path)?;

    shell_integration::add_to_paths(&out_path)?;
    message!(message_callback, "Added '{}' to PATH", &out_path.to_string_lossy());

    message!(message_callback, "Searching for executable");
    if let Some(exec_path) = file_permissions::find_executable(&out_path, &package.name) {
        let exec_name = exec_path.file_name().unwrap().to_string_lossy();
        file_permissions::make_executable(&exec_path)?;
        message!(message_callback, "Made executable: {}", exec_name);
        package.exec_path = Some(exec_path.clone());
        message!(message_callback, "Added executable permission for '{}'", exec_name);
    } else {
        message!(message_callback, "Could not automatically locate executable");
        package.exec_path = None;
    }

    package.install_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();
    Ok(package)
}

fn handle_compressed<H>(
    asset_path: &Path,
    cache_path: &Path,
    package: Package,
    message_callback: &mut Option<H>,
) -> Result<Package>
where
    H: FnMut(&str),
{
    message!(message_callback, "Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy());
    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;
    handle_file(&extracted_path, package, message_callback)
}

fn handle_appimage<H>(
    asset_path: &Path,
    package: Package,
    message_callback: &mut Option<H>,
) -> Result<Package>
where
    H: FnMut(&str),
{
    // TODO: logic that unpacks appimage to get app icon/name
    handle_file(asset_path, package, message_callback)
}

fn handle_file<H>(
    asset_path: &Path,
    mut package: Package,
    message_callback: &mut Option<H>,
) -> Result<Package>
where
    H: FnMut(&str),
{
    let filename = asset_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.binaries_dir.join(filename);

    message!(message_callback, "Moving file to '{}' ...", out_path.to_string_lossy());
    if let Err(_) = fs::rename(asset_path, &out_path) {
        fs::copy(asset_path, &out_path)?;
        fs::remove_file(asset_path)?;
    }

    file_permissions::make_executable(&out_path)?;
    message!(message_callback, "Made '{}' executable", out_path.file_name().unwrap().to_string_lossy());

    symlink_handling::add_link(&out_path, &package.name)?;
    message!(message_callback, "Created symlink: {} → {}", asset_path.to_string_lossy(), out_path.to_string_lossy());

    package.install_path = Some(out_path.clone());
    package.exec_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();
    Ok(package)
}
