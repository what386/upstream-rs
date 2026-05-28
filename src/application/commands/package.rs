use crate::{
    application::operations::metadata_operation::MetadataManager,
    application::output::{self, Status},
    services::integration::SymlinkManager,
    services::storage::{metadata_storage::MetadataStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Result, anyhow};

pub fn run_pin(name: String, reason: Option<String>) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage, &mut metadata_storage);

    println!("{}", output::title("Package pin"));

    package_manager.pin_package(&name, reason)?;
    output::status_line(Status::Ok, &name, "pinned");

    Ok(())
}

pub fn run_unpin(name: String) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage, &mut metadata_storage);

    println!("{}", output::title("Package unpin"));

    package_manager.unpin_package(&name)?;
    output::status_line(Status::Ok, &name, "unpinned");

    Ok(())
}

pub fn run_remove(name: String) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage, &mut metadata_storage);

    println!("{}", output::title("Package metadata remove"));

    package_manager.remove_package(&name)?;
    output::status_line(Status::Ok, &name, "metadata removed");

    Ok(())
}

pub fn run_set_key(name: String, keys: Vec<String>) -> Result<()> {
    if keys.is_empty() {
        return Err(anyhow!("At least one metadata assignment is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage, &mut metadata_storage);

    println!("{}", output::title("Package metadata set"));

    if keys.len() > 1 {
        let results = package_manager.set_bulk(&name, &keys);
        for applied in &results.applied {
            output::status_line(
                Status::Ok,
                &applied.key,
                format!("set to '{}'", applied.value),
            );
        }
        for (key, err) in &results.failures {
            output::status_line(Status::Fail, key, err);
        }
    } else {
        let applied = package_manager.set_key(&name, &keys[0])?;
        output::status_line(
            Status::Ok,
            &applied.key,
            format!("set to '{}'", applied.value),
        );
    }

    println!("{}", output::success("Package metadata saved."));

    Ok(())
}

pub fn run_get_key(name: String, keys: Vec<String>) -> Result<()> {
    if keys.is_empty() {
        return Err(anyhow!("At least one metadata key is required"));
    }

    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let package_manager = MetadataManager::new(&mut package_storage, &mut metadata_storage);

    println!("{}", output::title("Package metadata get"));

    if keys.len() > 1 {
        let results = package_manager.get_bulk(&name, &keys);
        if results.values.is_empty() {
            println!("{}", output::warning("No values found."));
        } else {
            for (key, value) in results.values {
                output::kv(&key, value);
            }
        }
        for (key, err) in results.failures {
            output::status_line(Status::Fail, key, err);
        }
    } else {
        let value = package_manager.get_key(&name, &keys[0])?;
        output::kv(&keys[0], value);
    }

    Ok(())
}

pub fn run_metadata(name: String) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let package_storage = PackageStorage::new(&paths.config.packages_file)?;

    let package = package_storage
        .get_package_by_name(&name)
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

    println!("{}", output::title(format!("Metadata for '{}'", name)));

    let json = serde_json::to_string_pretty(package)?;
    println!("{}", json);

    Ok(())
}

pub fn run_rename(old_name: String, new_name: String) -> Result<()> {
    let paths = UpstreamPaths::new()?;
    let mut package_storage = PackageStorage::new(&paths.config.packages_file)?;
    let package_before = package_storage
        .get_package_by_name(&old_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", old_name))?;

    let mut metadata_storage = MetadataStorage::new(&paths.config.metadata_file)?;
    let mut package_manager = MetadataManager::new(&mut package_storage, &mut metadata_storage);
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
