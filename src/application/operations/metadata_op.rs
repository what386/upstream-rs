use crate::storage::database::PackageDatabase;
use anyhow::Result;

pub struct MetadataManager<'a> {
    package_database: &'a mut PackageDatabase,
}

impl<'a> MetadataManager<'a> {
    pub fn new(package_database: &'a mut PackageDatabase) -> Self {
        Self { package_database }
    }

    /// Pins a package to its current version, preventing automatic updates.
    pub fn pin_package(&mut self, name: &str) -> Result<()> {
        self.package_database.update_package(name, |package| {
            if package.is_pinned {
                return Ok(false);
            }
            package.is_pinned = true;
            Ok(true)
        })?;
        Ok(())
    }

    /// Unpins a package, allowing it to receive automatic updates.
    pub fn unpin_package(&mut self, name: &str) -> Result<()> {
        self.package_database.update_package(name, |package| {
            if !package.is_pinned {
                return Ok(false);
            }
            package.is_pinned = false;
            Ok(true)
        })?;
        Ok(())
    }

    /// Renames a package alias without changing provider/repo/version metadata.
    pub fn rename_package(&mut self, old_name: &str, new_name: &str) -> Result<bool> {
        let old_name = old_name.trim();
        let new_name = new_name.trim();

        if old_name.is_empty() || new_name.is_empty() {
            return Err(anyhow::anyhow!("Package names cannot be empty"));
        }

        if old_name == new_name {
            return Ok(false);
        }

        if self.package_database.get_package(new_name)?.is_some() {
            return Err(anyhow::anyhow!("Package '{}' already exists", new_name));
        }

        self.package_database.rename_package(old_name, new_name)?;

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::MetadataManager;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::storage::database::PackageDatabase;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_packages_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-metadata-test-{name}-{nanos}"))
            .join("packages.json")
    }

    fn test_package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn pin_and_unpin_update_package_state() {
        let path = temp_packages_file("pin");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        let mut storage = PackageDatabase::open(&path).expect("create storage");
        let package = test_package("fd");
        storage.upsert_package(&package).expect("store package");
        let mut manager = MetadataManager::new(&mut storage);

        manager.pin_package("fd").expect("pin package");
        assert!(
            manager
                .package_database
                .get_package("fd")
                .expect("load package")
                .expect("package")
                .is_pinned
        );

        manager.unpin_package("fd").expect("unpin package");
        assert!(
            !manager
                .package_database
                .get_package("fd")
                .expect("load package")
                .expect("package")
                .is_pinned
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn rename_package_rejects_duplicates_and_updates_alias() {
        let path = temp_packages_file("rename");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        let mut storage = PackageDatabase::open(&path).expect("create storage");
        let old = test_package("old");
        storage.upsert_package(&old).expect("store old");
        let taken = test_package("taken");
        storage.upsert_package(&taken).expect("store taken");
        let mut manager = MetadataManager::new(&mut storage);

        assert!(manager.rename_package("old", "taken").is_err());
        manager
            .rename_package("old", "new")
            .expect("rename package");
        assert!(
            manager
                .package_database
                .get_package("new")
                .expect("load new")
                .is_some()
        );
        assert!(
            manager
                .package_database
                .get_package("old")
                .expect("load old")
                .is_none()
        );

        cleanup(&path).expect("cleanup");
    }
}
