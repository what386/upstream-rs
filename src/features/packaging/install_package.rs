use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::models::common::enums::Filetype;
use crate::models::upstream::{Package, Repository};

use crate::services::providers::provider_manager::ProviderManager;
use crate::services::filesystem::{file_decompressor, symlink_handling, file_permissions, shell_integration};
use crate::utils::upstream_paths::PATHS;

pub async fn perform_install(
    package: Package,
    repository: Repository,
    provider_manager: &ProviderManager,
    progress: Option<&mut dyn FnMut(u64, u64)>) -> Result<Package> {

    let latest_release = provider_manager.get_latest_release(&repository.slug, &repository.provider).await?;
    let best_asset = provider_manager.find_recommended_asset(&latest_release, &package)?;
    let download_path = provider_manager.download_asset(&best_asset, &repository.provider , progress).await?;

    let installed_package = match package.pkg_kind {
        Filetype::AppImage => handle_appimage(&download_path, package),
        Filetype::Compressed => handle_compressed(&download_path, package),
        Filetype::Archive => handle_archive(&download_path, package),
        _ => handle_binary(&download_path, package)
    }?;

    Ok(installed_package)
}

fn handle_compressed(asset_path: &Path, package: Package) -> Result<Package> {
    let cache_path = std::env::temp_dir().join("upstream_extraction");
    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;

    handle_binary(&extracted_path, package)
}

fn handle_archive(asset_path: &Path, mut package: Package) -> Result<Package> {
    let cache_path = std::env::temp_dir().join("upstream_extraction");
    let extracted_path = file_decompressor::decompress(asset_path, &cache_path)?;

    let dirname = extracted_path.file_name() // this is a directory actually
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

    let out_path = PATHS.archives_dir.join(dirname);

    fs::rename(extracted_path, &out_path)?;

    shell_integration::add_to_paths(&out_path)?;

    if let Some(exec_path) = file_permissions::find_executable(&out_path, &package.name) {
        file_permissions::make_executable(&exec_path)?;
        package.exec_path = Some(exec_path);
    } else {
        // is okay, user can update later
        package.exec_path = None;
    }

    package.install_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(package)
}

fn handle_appimage(asset_path: &Path, mut package: Package) -> Result<Package> {
    // TODO: logic that unpacks appimage
    // to automatically get app icon

    let filename = asset_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

    let out_path = PATHS.appimages_dir.join(filename);

    fs::rename(asset_path, &out_path)?;

    symlink_handling::add_link(&out_path, &package.name)?;
    file_permissions::make_executable(&out_path)?;

    package.install_path = Some(out_path.clone());
    package.exec_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(package)
}

fn handle_binary(asset_path: &Path, mut package: Package) -> Result<Package> {
    let filename = asset_path.file_name()
        .ok_or_else(|| anyhow!("Invalid path: no filename"))?;

    let out_path = PATHS.binaries_dir.join(filename);

    fs::rename(asset_path, &package.name)?;
    symlink_handling::add_link(&out_path, &package.name)?;
    file_permissions::make_executable(&out_path)?;

    package.install_path = Some(out_path.clone());
    package.exec_path = Some(out_path.clone());
    package.last_upgraded = Utc::now();

    Ok(package)
}
