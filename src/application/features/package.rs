use crate::{
    application::operations::package_metadata::MetadataManager,
    services::storage::package_storage::PackageStorage,
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
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut package_storage_mut = package_storage;
    let package_manager = MetadataManager::new(&mut package_storage_mut);

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
