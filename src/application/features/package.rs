use crate::{
    application::operations::metadata_operation::MetadataManager,
    services::integration::SymlinkManager, services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
use anyhow::Result;

pub fn run_pin(name: String) -> Result<()> {
    let paths = UpstreamPaths::new();
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage);

    let mut message_callback = Some(move |msg: &str| {
        println!("{}", msg);
    });

    package_manager.pin_package(&name, &mut message_callback)?;
    println!("Package '{}' has been pinned", name);

    Ok(())
}

pub fn run_unpin(name: String) -> Result<()> {
    let paths = UpstreamPaths::new();
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage);

    let mut message_callback = Some(move |msg: &str| {
        println!("{}", msg);
    });

    package_manager.unpin_package(&name, &mut message_callback)?;
    println!("Package '{}' has been unpinned", name);

    Ok(())
}

pub fn run_set_key(name: String, keys: Vec<String>) -> Result<()> {
    let paths = UpstreamPaths::new();
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage);

    let mut message_callback = Some(move |msg: &str| {
        println!("{}", msg);
    });

    if keys.len() > 1 {
        package_manager.set_bulk(&name, &keys, &mut message_callback)?;
    } else {
        package_manager.set_key(&name, &keys[0], &mut message_callback)?;
    }

    println!("Package metadata saved!");

    Ok(())
}

pub fn run_get_key(name: String, keys: Vec<String>) -> Result<()> {
    let paths = UpstreamPaths::new();
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let package_manager = MetadataManager::new(&mut package_storage);

    let mut message_callback = Some(move |msg: &str| {
        println!("{}", msg);
    });

    if keys.len() > 1 {
        let results = package_manager.get_bulk(&name, &keys, &mut message_callback)?;
        if results.is_empty() {
            println!("No values found");
        }
    } else {
        package_manager.get_key(&name, &keys[0], &mut message_callback)?;
    }

    Ok(())
}

pub fn run_metadata(name: String) -> Result<()> {
    let paths = UpstreamPaths::new();
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let package = package_storage
        .get_package_by_name(&name)
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

    println!("Metadata for package '{}':", name);
    println!();

    let json = serde_json::to_string_pretty(package)?;
    println!("{}", json);

    Ok(())
}

pub fn run_rename(old_name: String, new_name: String) -> Result<()> {
    let paths = UpstreamPaths::new();
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let package_before = package_storage
        .get_package_by_name(&old_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", old_name))?;

    let mut package_manager = MetadataManager::new(&mut package_storage);
    let mut message_callback = Some(move |msg: &str| {
        println!("{}", msg);
    });

    package_manager.rename_package(&old_name, &new_name, &mut message_callback)?;

    if let Some(exec_path) = package_before.exec_path.as_ref() {
        let symlink_manager = SymlinkManager::new(&paths.integration.symlinks_dir);
        let mut created_new = false;

        if let Err(err) = symlink_manager.add_link(exec_path, &new_name) {
            println!(
                "Warning: package was renamed, but failed to create new symlink '{}': {}",
                new_name, err
            );
        } else {
            created_new = true;
        }

        if created_new && let Err(err) = symlink_manager.remove_link(&old_name) {
            println!(
                "Warning: package was renamed, but failed to remove old symlink '{}': {}",
                old_name, err
            );
        }
    }

    println!("Package '{}' has been renamed to '{}'", old_name, new_name);
    Ok(())
}
