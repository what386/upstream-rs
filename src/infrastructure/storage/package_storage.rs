use std::fs;
use std::io;
use std::path::Path;

use crate::models::upstream::Package;

pub struct PackageStorage {
    packages_file_path: String,
    packages: Vec<Package>,
}

impl PackageStorage {
    pub fn new(packages_file_path: String) -> io::Result<Self> {
        let mut storage = Self {
            packages_file_path,
            packages: Vec::new(),
        };
        storage.load_packages()?;
        Ok(storage)
    }

    /// Load all packages from the packages.json file.
    pub fn load_packages(&mut self) -> io::Result<()> {
        if !Path::new(&self.packages_file_path).exists() {
            self.packages = Vec::new();
            return Ok(());
        }

        match fs::read_to_string(&self.packages_file_path) {
            Ok(json) => {
                self.packages = serde_json::from_str(&json).unwrap_or_default();
                Ok(())
            }
            Err(e) => {
                eprintln!("Warning: Failed to load packages: {}", e);
                self.packages = Vec::new();
                Ok(())
            }
        }
    }

    /// Save all packages to the packages.json file.
    pub fn save_packages(&self) -> io::Result<()> {
        let json = serde_json::to_string_pretty(&self.packages)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to serialize: {}", e)))?;

        fs::write(&self.packages_file_path, json)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to save packages: {}", e)))
    }

    /// Get all stored packages.
    pub fn get_all_packages(&self) -> &[Package] {
        &self.packages
    }

    /// Get a package by name.
    pub fn get_package_by_name(&self, name: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Get a package by package ID.
    pub fn get_package_by_id(&self, id: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.id == id)
    }

    /// Add or update a package in the repository.
    pub fn add_or_update_package(&mut self, package: Package) -> io::Result<()> {
        self.packages.retain(|p| p.id != package.id);
        self.packages.push(package);
        self.save_packages()
    }

    /// Remove a package from the repository.
    pub fn remove_package(&mut self, package: &Package) -> io::Result<bool> {
        let initial_len = self.packages.len();
        self.packages.retain(|p| !p.is_same(package));

        if self.packages.len() < initial_len {
            self.save_packages()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Remove a package from the repository by ID.
    pub fn remove_package_by_id(&mut self, id: &str) -> io::Result<bool> {
        let initial_len = self.packages.len();
        self.packages.retain(|p| p.id != id);

        if self.packages.len() < initial_len {
            self.save_packages()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Remove a package from the repository by name.
    pub fn remove_package_by_name(&mut self, name: &str) -> io::Result<bool> {
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
