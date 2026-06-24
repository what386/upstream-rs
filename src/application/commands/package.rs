#[cfg(target_os = "linux")]
use crate::services::artifact::AppImageExtractor;
use crate::{
    application::operations::metadata_op::MetadataManager,
    models::upstream::Package,
    output::{self, Status},
    services::integration::{DesktopManager, SymlinkManager},
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result};

pub fn run_pin(name: String) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut package_manager = MetadataManager::new(&mut package_database);

    println!("{}", output::title("Package pin"));

    package_manager.pin_package(&name)?;
    output::status_line(Status::Ok, &name, "pinned");

    Ok(())
}

pub fn run_unpin(name: String) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let mut package_manager = MetadataManager::new(&mut package_database);

    println!("{}", output::title("Package unpin"));

    package_manager.unpin_package(&name)?;
    output::status_line(Status::Ok, &name, "unpinned");

    Ok(())
}

pub fn run_rename(old_name: String, new_name: String) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let package_before = package_database
        .get_package(&old_name)?
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", old_name))?;

    let mut package_manager = MetadataManager::new(&mut package_database);
    println!("{}", output::title("Package rename"));

    let renamed = package_manager.rename_package(&old_name, &new_name)?;
    if !renamed {
        output::status_line(Status::Skip, &old_name, "old and new names are identical");
        return Ok(());
    }

    if let Some(exec_path) = package_before.exec_path.as_ref() {
        let symlink_manager = SymlinkManager::new(&paths.integration.symlinks_dir);
        let mut created_new = false;

        if let Err(err) = symlink_manager.add_link(exec_path, &new_name) {
            println!(
                "{}",
                output::warning(format!(
                    "Renamed package but failed to create new symlink '{}': {}",
                    new_name, err
                ))
            );
        } else {
            created_new = true;
        }

        if created_new && let Err(err) = symlink_manager.remove_link(&old_name) {
            println!(
                "{}",
                output::warning(format!(
                    "Renamed package but failed to remove old symlink '{}': {}",
                    old_name, err
                ))
            );
        }
    }

    println!(
        "{}",
        output::success(format!("Package '{}' renamed to '{}'.", old_name, new_name))
    );
    Ok(())
}

pub async fn run_add_entry(name: String) -> Result<()> {
    let (paths, mut package_database, mut package) = load_installed_package(&name)?;

    #[cfg(target_os = "linux")]
    let appimage_extractor =
        AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

    #[cfg(target_os = "linux")]
    let desktop_manager = DesktopManager::new(&paths, &appimage_extractor);
    #[cfg(not(target_os = "linux"))]
    let desktop_manager = DesktopManager::new(&paths);

    println!("{}", output::title("Package add-entry"));

    let mut ignored_messages = Some(|_: &str| {});
    desktop_manager
        .enable_package_entry(&mut package, &mut ignored_messages)
        .await?;

    save_package(&mut package_database, &package)?;
    output::status_line(Status::Ok, &name, "entry added");

    Ok(())
}

pub async fn run_rm_entry(name: String) -> Result<()> {
    let (paths, mut package_database, mut package) = load_installed_package(&name)?;

    #[cfg(target_os = "linux")]
    let appimage_extractor =
        AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

    #[cfg(target_os = "linux")]
    let desktop_manager = DesktopManager::new(&paths, &appimage_extractor);
    #[cfg(not(target_os = "linux"))]
    let desktop_manager = DesktopManager::new(&paths);

    println!("{}", output::title("Package rm-entry"));

    let mut ignored_messages = Some(|_: &str| {});
    desktop_manager.disable_package_entry(&mut package, &mut ignored_messages)?;

    save_package(&mut package_database, &package)?;
    output::status_line(Status::Ok, &name, "entry removed");

    Ok(())
}

fn load_installed_package(name: &str) -> Result<(UpstreamPaths, PackageDatabase, Package)> {
    let paths = UpstreamPaths::new()?;
    let package_database = PackageDatabase::open(&paths.config.packages_database_file)?;
    let package = package_database
        .get_package(name)?
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

    Ok((paths, package_database, package))
}

fn save_package(package_database: &mut PackageDatabase, package: &Package) -> Result<()> {
    package_database.upsert_package(package).context(format!(
        "Failed to save package '{}' to storage",
        package.name
    ))
}
