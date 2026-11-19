use crate::{storage::database::PackageDatabase, utils::static_paths::UpstreamPaths};
use anyhow::Result;

pub struct MetadataManager<'a> {
    package_database: &'a mut PackageDatabase,
    paths: &'a UpstreamPaths,
}

impl<'a> MetadataManager<'a> {
    pub fn new(package_database: &'a mut PackageDatabase, paths: &'a UpstreamPaths) -> Self {
        Self {
            package_database,
            paths,
        }
    }

    /// Pins a package to its current version, preventing automatic updates.
    pub fn pin_package(&mut self, name: &str) -> Result<()> {
        self.set_pinned(name, true)
    }

    /// Unpins a package, allowing it to receive automatic updates.
    pub fn unpin_package(&mut self, name: &str) -> Result<()> {
        self.set_pinned(name, false)
    }

    fn set_pinned(&mut self, name: &str, pinned: bool) -> Result<()> {
        let mut changed_id = None;
        self.package_database.update_package(name, |package| {
            if package.is_pinned == pinned {
                return Ok(false);
            }
            package.is_pinned = pinned;
            changed_id = Some(package.id.clone());
            Ok(true)
        })?;
        if let Some(id) = changed_id {
            crate::storage::updates::invalidate(self.paths, &id);
        }
        Ok(())
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
        let paths = crate::utils::test_support::upstream_paths(path.parent().unwrap());
        let mut manager = MetadataManager::new(&mut storage, &paths);

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
}
