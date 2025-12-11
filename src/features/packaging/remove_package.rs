use std::fs;

use anyhow::{Result, anyhow};

use crate::models::upstream::Package;

use crate::services::filesystem::{symlink_handling, shell_integration};

macro_rules! emit {
    ($msg:expr, $cb:expr) => {{
        if let Some(cb) = $cb {
            cb($msg);
        }
    }};
}

pub async fn perform_remove(
    mut package: Package,
    message: &mut Option<&mut dyn FnMut(String)>
) -> Result<Package> {
    let install_path = package.install_path.as_ref()
        .ok_or_else(|| anyhow!("Package is not installed"))?;

    shell_integration::remove_from_paths(install_path)?;
    emit!(format!("Removing '{}' from PATH", &install_path.to_string_lossy()), message);

    symlink_handling::remove_link(&package.name)?;
    emit!(format!("Removed symlink for '{}'", package.name), message);

    if install_path.is_dir() {
        emit!(format!("Removing directory: {}", install_path.to_string_lossy()), message);
        fs::remove_dir_all(install_path)?;
    } else if install_path.is_file() {
        emit!(format!("Removing file: {}", install_path.to_string_lossy()), message);
        fs::remove_file(install_path)?;

    } else {
        return Err(anyhow!("Install path is invalid: {}", install_path.to_string_lossy()));
    }

    package.exec_path = None;
    package.install_path = None;

    Ok(package)
}
