use std::{fs, io};
use std::path::{PathBuf, Path};

use anyhow::{Result, anyhow};

use crate::models::upstream::Package;

pub struct PackageStorage {
    packages: Vec<Package>,
    packages_file: PathBuf
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
    fn load_packages(&mut self) -> Result<()> {
        if !&self.packages_file.exists() {
            self.packages = Vec::new();
            return Ok(());
        }

        match fs::read_to_string(&self.packages_file) {
            Ok(json) => {
                self.packages = serde_json::from_str(&json).unwrap_or_default();
                Ok(())
            }
            Err(e) => {
                Err(anyhow!("Warning: Failed to load packages: {}", e))
            }
        }
    }

    /// Save all packages to the packages.json file.
    pub fn save_packages(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.packages)
            .map_err(|e| io::Error::other(e.to_string()))?;

        fs::write(&self.packages_file, json)
            .map_err(|e| io::Error::other(e.to_string()))?;

        Ok(())
    }

    /// Get all stored packages.
    pub fn get_all_packages(&self) -> &[Package] {
        &self.packages
    }

    /// Get all stored packages. (mutable)
    pub fn get_mut_all_packages(&mut self) -> &mut [Package] {
        &mut self.packages
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

    /// Remove a package from the repository.
    pub fn remove_package(&mut self, package: &Package) -> Result<bool> {
        let initial_len = self.packages.len();
        self.packages.retain(|p| !p.is_same_as(package));

        if self.packages.len() < initial_len {
            self.save_packages()?;
            Ok(true)
        } else {
            Ok(false)
        }
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
