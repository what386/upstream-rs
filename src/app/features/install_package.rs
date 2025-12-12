use std::fs;
use std::path::Path;

use tokio;
use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::models::common::Version;
use crate::models::common::enums::{Channel, Filetype, Provider};
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

pub async fn install(
    credentials: Credentials,
    name: String,
    repository: String,
    filetype: Filetype,
    channel: Channel,
    provider: Provider,
    progress: Option<&mut dyn FnMut(u64, u64)>,
    message_cb: &mut Option<&mut dyn FnMut(&str)>
) -> Result<Package> {
    let temp_path = std::env::temp_dir().join("upstream");
    let download_cache = temp_path.join("downloads");
    let extraction_cache = temp_path.join("extracts");

    fs::create_dir_all(&download_cache)?;
    fs::create_dir_all(&extraction_cache)?;

    let provider_manager = ProviderManager::new(credentials, &download_cache)?;
    let mut package_store = PackageStorage::new()?;

    let package = Package::with_defaults(
        name,
        repository,
        filetype,
        Version::new(0, 0, 0, false),
        channel,
        provider);

    let installed_package = perform_installation(
        package,
        &provider_manager,
        &extraction_cache,
        progress,
        message_cb
    ).await?;

    package_store.add_or_update_package(&installed_package)?;

    tokio::task::spawn_blocking(move || fs::remove_dir_all(&temp_path))
        .await??;

    emit!(message_cb, "Package installed!");
    Ok(installed_package)
}

async fn perform_installation(
    mut package: Package,
    provider_manager: &ProviderManager,
    extract_cache: &Path,
    progress: Option<&mut dyn FnMut(u64, u64)>,
    message_cb: &mut Option<&mut dyn FnMut(&str)>
) -> Result<Package> {
    if package.install_path != None {
        return Err(anyhow!("Package '{}' is already installed", package.name));
    }

    emit!(message_cb, "Fetching latest release ...");
    let latest_release = provider_manager.get_latest_release(&package.repo_slug, &package.provider).await?;

    package.version = latest_release.version.clone();

    emit!(message_cb, "Selecting asset from '{}'", latest_release.name);
    let best_asset = provider_manager.find_recommended_asset(&latest_release, &package)?;

    emit!(message_cb, "Downloading '{}' ...", best_asset.name);
    let download_path = provider_manager.download_asset(&best_asset, &package.provider , progress).await?;

    emit!(message_cb, "Installing package ...");
    let installed_package = match package.filetype {
        Filetype::AppImage => handle_appimage(&download_path, package, message_cb),
        Filetype::Compressed => handle_compressed(&download_path, extract_cache, package, message_cb),
        Filetype::Archive => handle_archive(&download_path, extract_cache, package, message_cb),
        _ => handle_file(&download_path, package, message_cb)
    }?;

    Ok(installed_package)
}

fn handle_archive(
    asset_path: &Path,
    cache_path: &Path,
    mut package: Package,
    message_cb: &mut Option<&mut dyn FnMut(&str)>
) -> Result<Package> {
    emit!(message_cb, "Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy());

    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;

    let dirname = extracted_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.archives_dir.join(dirname);

    emit!(message_cb, "Moving directory to '{}' ...", out_path.to_string_lossy());
    fs::rename(extracted_path, &out_path)?;

    shell_integration::add_to_paths(&out_path)?;
    emit!(message_cb, "Added '{}' to PATH", &out_path.to_string_lossy());

    emit!(message_cb, "Searching for executable");
    if let Some(exec_path) = file_permissions::find_executable(&out_path, &package.name) {
        let exec_name = exec_path.file_name().unwrap().to_string_lossy();
        emit!(message_cb, "Found executable: {}", exec_name);

        file_permissions::make_executable(&exec_path)?;
        package.exec_path = Some(exec_path.clone());
        emit!(message_cb, "Added executable permission for '{}'", exec_name);
    } else {
        emit!(message_cb, "Could not automatically locate executable");
        package.exec_path = None;
    }
    package.install_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();
    Ok(package)
}

fn handle_compressed(asset_path: &Path, cache_path: &Path, package: Package, message_cb: &mut Option<&mut dyn FnMut(&str)>) -> Result<Package> {
    emit!(message_cb, "Extracting '{}' ...", asset_path.file_name().unwrap().to_string_lossy());
    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;

    handle_file(&extracted_path, package, message_cb)
}

fn handle_appimage(asset_path: &Path, mut package: Package, message_cb: &mut Option<&mut dyn FnMut(&str)>) -> Result<Package> {
    // TODO: logic that unpacks appimage to get app icon/name

    handle_file(asset_path, package, message_cb)
}

fn handle_file(asset_path: &Path, mut package: Package, message_cb: &mut Option<&mut dyn FnMut(&str)>) -> Result<Package> {
    let filename = asset_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
    let out_path = PATHS.binaries_dir.join(filename);

    emit!(message_cb, "Moving file to '{}' ...", out_path.to_string_lossy());

    if let Err(_) = fs::rename(asset_path, &out_path) {
        fs::copy(asset_path, &out_path)?;
        fs::remove_file(asset_path)?;
    }

    file_permissions::make_executable(&out_path)?;
    emit!(message_cb, "Made '{}' executable", out_path.file_name().unwrap().to_string_lossy());

    symlink_handling::add_link(&out_path, &package.name)?;
    emit!(message_cb, "Created symlink: {} → {}", asset_path.to_string_lossy(), out_path.to_string_lossy());

    package.install_path = Some(out_path.clone());
    package.exec_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(package)
}
