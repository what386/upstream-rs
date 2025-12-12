use std::fs;

use anyhow::{Result, anyhow};

use crate::models::upstream::Package;

use crate::services::filesystem::{symlink_handling, shell_integration};
use crate::services::storage::package_storage::PackageStorage;

macro_rules! emit {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb {
            cb(&format!($($arg)*));
        }
    }};
}

pub async fn remove(
    package_name: String,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    let mut package_store = PackageStorage::new()?;

    let package = package_store.get_mut_package_by_name(&package_name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", package_name))?;

    perform_remove(package, message).await?;

    emit!(message, "Package installed!");
    Ok(())
}

pub async fn perform_remove(
    package: &mut Package,
    message: &mut Option<&mut dyn FnMut(&str)>
) -> Result<()> {
    let install_path = package.install_path.as_ref()
        .ok_or_else(|| anyhow!("Package is not installed"))?;

    shell_integration::remove_from_paths(install_path)?;
    emit!(message, "Removing '{}' from PATH", install_path.to_string_lossy());

    symlink_handling::remove_link(&package.name)?;
    emit!(message, "Selecting asset from '{}'", package.name);

    if install_path.is_dir() {
        emit!(message, "Removing directory: {}", install_path.to_string_lossy());
        fs::remove_dir_all(install_path)?;
    } else if install_path.is_file() {
        emit!(message, "Removing file: {}", install_path.to_string_lossy());
        fs::remove_file(install_path)?;

    } else {
        return Err(anyhow!("Install path is invalid: {}", install_path.to_string_lossy()));
    }

    package.exec_path = None;
    package.install_path = None;

    Ok(())
}
