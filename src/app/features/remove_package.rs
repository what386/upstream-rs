use std::fs;
use anyhow::{Result, anyhow};

use crate::models::upstream::{Package};
use crate::services::filesystem::{symlink_handling, shell_integration};
use crate::services::storage::package_storage::PackageStorage;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub fn remove_bulk<H, G>(
    package_names: Vec<String>,
    message_callback: &mut Option<H>,
    overall_progress_callback: &mut Option<G>,
) -> Result<()>
where
    H: FnMut(&str),
    G: FnMut(u32, u32),
{
    let total = package_names.len() as u32;
    let mut completed = 0;
    let mut failures = 0;

    for package_name in package_names {
        message!(message_callback, "Removing '{}' ...", package_name);

        match remove_single(
            package_name,
            message_callback,
        ) {
            Ok(_) => message!(message_callback, "Package removed!"),
            Err(e) => {
                message!(message_callback, "Removal failed: {}", e);
                failures += 1;
            }
        }

        completed += 1;
        if let Some(cb) = overall_progress_callback.as_mut() {
            cb(completed, total);
        }
    }

    if failures > 0 {
        message!(message_callback, "{} package(s) failed to be removed", failures);
    }

    Ok(())
}

pub fn remove_single<H>(
    package_name: String,
    message_callback: &mut Option<H>,
) -> Result<()>
where
    H: FnMut(&str),
{
    let mut package_store = PackageStorage::new()?;

    let package = package_store.get_mut_package_by_name(&package_name)
        .ok_or_else(|| anyhow!("Package '{}' is not installed.", package_name))?;

    perform_remove(package, message_callback)?;

    package_store.save_packages()?;

    Ok(())
}

pub fn perform_remove<H>(
    package: &mut Package,
    message_callback: &mut Option<H>,
) -> Result<()>
where
    H: FnMut(&str),
{
    let install_path = package.install_path.as_ref()
        .ok_or_else(|| anyhow!("Package '{}' is not installed", package.name))?;

    message!(message_callback, "Removing '{}' from PATH ...", install_path.to_string_lossy());
    shell_integration::remove_from_paths(install_path)?;

    message!(message_callback, "Removing symlink for '{}'", package.name);
    symlink_handling::remove_link(&package.name)?;

    if install_path.is_dir() {
        message!(message_callback, "Removing directory: {}", install_path.to_string_lossy());
        fs::remove_dir_all(install_path)?;
    } else if install_path.is_file() {
        message!(message_callback, "Removing file: {}", install_path.to_string_lossy());
        fs::remove_file(install_path)?;
    } else {
        return Err(anyhow!("Install path is invalid: {}", install_path.to_string_lossy()));
    }

    package.install_path = None;
    package.exec_path = None;

    Ok(())
}

