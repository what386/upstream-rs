use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::models::upstream::Package;
use crate::utils::filesystem::atomic_ops::write_atomic;

pub struct PackageStorage {
    packages: Vec<Package>,
    packages_file: PathBuf,
}

impl PackageStorage {
    pub fn new(packages_file: &Path) -> Result<Self> {
        let mut storage = Self {
            packages: Vec::new(),
            packages_file: packages_file.to_path_buf(),
        };

        storage.load_packages()?;

        Ok(storage)
    }

    /// Load all packages from the packages.json file.
    pub fn load_packages(&mut self) -> Result<()> {
        if !&self.packages_file.exists() {
            self.packages = Vec::new();
            return Ok(());
        }

        match fs::read_to_string(&self.packages_file) {
            Ok(json) => {
                if json.trim().is_empty() {
                    self.packages = Vec::new();
                    return Ok(());
                }

                self.packages = serde_json::from_str(&json).with_context(|| {
                    format!(
                        "Failed to parse package storage '{}'. The file may be corrupt; restore from backup or fix JSON syntax",
                        self.packages_file.display()
                    )
                })?;
                Ok(())
            }
            Err(e) => Err(anyhow!("Warning: Failed to load packages: {}", e)),
        }
    }

    /// Save all packages to the packages.json file.
    pub fn save_packages(&self) -> Result<()> {
        let json =
            serde_json::to_string_pretty(&self.packages).context("Failed to serialize packages")?;

        write_atomic(&self.packages_file, json.as_bytes()).with_context(|| {
            format!(
                "Failed to write package storage to '{}'",
                self.packages_file.display()
            )
        })?;

        Ok(())
    }

    /// Get all stored packages.
    pub fn get_all_packages(&self) -> &[Package] {
        &self.packages
    }

    /// Get a package by name.
    pub fn get_package_by_name(&self, name: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// get a package by mame. (mutable)
    pub fn get_mut_package_by_name(&mut self, name: &str) -> Option<&mut Package> {
        self.packages.iter_mut().find(|p| p.name == name)
    }

    /// Add or update a package in the repository.
    pub fn add_or_update_package(&mut self, package: Package) -> Result<()> {
        self.packages.retain(|p| !p.is_same_as(&package));
        self.packages.push(package);
        self.save_packages()
    }

    /// Remove a package from the repository by name.
    pub fn remove_package_by_name(&mut self, name: &str) -> Result<bool> {
        let initial_len = self.packages.len();
        self.packages.retain(|p| p.name != name);

        if self.packages.len() < initial_len {
            self.save_packages()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PackageStorage;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_packages_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-packages-test-{name}-{nanos}"))
            .join("packages.json")
    }

    fn test_package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
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
    fn new_starts_empty_when_file_missing() {
        let path = temp_packages_file("missing");
        let storage = PackageStorage::new(&path).expect("create storage");
        assert!(storage.get_all_packages().is_empty());
    }

    #[test]
    fn add_or_update_replaces_existing_identity_match() {
        let path = temp_packages_file("update");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut storage = PackageStorage::new(&path).expect("create storage");

        let mut first = test_package("tool");
        first.version.major = 1;
        storage
            .add_or_update_package(first.clone())
            .expect("store first");

        let mut second = first.clone();
        second.version.major = 2;
        storage
            .add_or_update_package(second.clone())
            .expect("store update");

        assert_eq!(storage.get_all_packages().len(), 1);
        assert_eq!(
            storage
                .get_package_by_name("tool")
                .expect("stored package")
                .version
                .major,
            2
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn remove_package_returns_expected_status() {
        let path = temp_packages_file("remove");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut storage = PackageStorage::new(&path).expect("create storage");
        storage
            .add_or_update_package(test_package("one"))
            .expect("store package");

        assert!(storage.remove_package_by_name("one").expect("remove"));
        assert!(
            !storage
                .remove_package_by_name("one")
                .expect("second remove")
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn invalid_json_file_returns_parse_error() {
        let path = temp_packages_file("invalid-json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, "{not-json").expect("write invalid json");
        let err = match PackageStorage::new(&path) {
            Ok(_) => panic!("invalid json should fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Failed to parse package storage"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn save_packages_creates_missing_parent_dirs() {
        let path = temp_packages_file("missing-parent");
        let mut storage = PackageStorage::new(&path).expect("create storage");
        storage
            .add_or_update_package(test_package("tool"))
            .expect("save package");

        assert!(path.exists());
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn save_packages_writes_valid_json_and_can_reload() {
        let path = temp_packages_file("reload");
        let mut storage = PackageStorage::new(&path).expect("create storage");
        storage
            .add_or_update_package(test_package("tool"))
            .expect("save package");

        let reloaded = PackageStorage::new(&path).expect("reload storage");
        assert_eq!(reloaded.get_all_packages().len(), 1);
        assert!(reloaded.get_package_by_name("tool").is_some());

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn save_packages_overwrites_atomically_visible_result() {
        let path = temp_packages_file("overwrite");
        let mut storage = PackageStorage::new(&path).expect("create storage");

        let mut first = test_package("tool");
        first.version.major = 1;
        storage.add_or_update_package(first).expect("save first");

        let mut second = test_package("tool");
        second.version.major = 2;
        storage.add_or_update_package(second).expect("save second");

        let json = fs::read_to_string(&path).expect("read final json");
        let decoded: Vec<Package> = serde_json::from_str(&json).expect("parse final json");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].version.major, 2);

        cleanup(&path).expect("cleanup");
    }
}
