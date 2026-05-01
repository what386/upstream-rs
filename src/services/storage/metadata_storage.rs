use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::models::upstream::PackageMetadata;
use crate::utils::filesystem::atomic_ops::write_atomic;

const METADATA_STORAGE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageMetadataFile {
    version: u32,
    packages: HashMap<String, PackageMetadata>,
}

impl Default for PackageMetadataFile {
    fn default() -> Self {
        Self {
            version: METADATA_STORAGE_VERSION,
            packages: HashMap::new(),
        }
    }
}

pub struct MetadataStorage {
    file: PackageMetadataFile,
    metadata_file: PathBuf,
}

impl MetadataStorage {
    pub fn new(metadata_file: &Path) -> Result<Self> {
        let mut storage = Self {
            file: PackageMetadataFile::default(),
            metadata_file: metadata_file.to_path_buf(),
        };
        storage.load()?;
        Ok(storage)
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.metadata_file.exists() {
            self.file = PackageMetadataFile::default();
            return Ok(());
        }

        match fs::read_to_string(&self.metadata_file) {
            Ok(json) => {
                if json.trim().is_empty() {
                    self.file = PackageMetadataFile::default();
                    return Ok(());
                }
                self.file = serde_json::from_str(&json).with_context(|| {
                    format!(
                        "Failed to parse metadata storage '{}'",
                        self.metadata_file.display()
                    )
                })?;
                Ok(())
            }
            Err(e) => Err(anyhow!("Warning: Failed to load metadata storage: {}", e)),
        }
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.file)
            .context("Failed to serialize metadata storage")?;
        write_atomic(&self.metadata_file, json.as_bytes()).with_context(|| {
            format!(
                "Failed to write metadata storage to '{}'",
                self.metadata_file.display()
            )
        })
    }

    pub fn set_pin_reason(&mut self, name: &str, reason: String) -> Result<()> {
        let entry = self.file.packages.entry(name.to_string()).or_default();
        entry.pin_reason = Some(reason);
        self.save()
    }

    pub fn clear_pin_reason(&mut self, name: &str) -> Result<()> {
        if let Some(entry) = self.file.packages.get_mut(name) {
            entry.pin_reason = None;
            if is_empty_entry(entry) {
                self.file.packages.remove(name);
            }
            self.save()?;
        }
        Ok(())
    }

    pub fn remove_package(&mut self, name: &str) -> Result<()> {
        if self.file.packages.remove(name).is_some() {
            self.save()?;
        }
        Ok(())
    }

    pub fn rename_package(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        if old_name == new_name {
            return Ok(());
        }
        if let Some(entry) = self.file.packages.remove(old_name) {
            self.file.packages.insert(new_name.to_string(), entry);
            self.save()?;
        }
        Ok(())
    }

    pub fn get_package(&self, name: &str) -> Option<&PackageMetadata> {
        self.file.packages.get(name)
    }
}

fn is_empty_entry(entry: &PackageMetadata) -> bool {
    entry.pin_reason.is_none()
}

#[cfg(test)]
mod tests {
    use super::MetadataStorage;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_metadata_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-meta-storage-test-{name}-{nanos}"))
            .join("metadata.json")
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn set_and_clear_pin_reason_round_trips() {
        let path = temp_metadata_file("set-clear");
        let mut storage = MetadataStorage::new(&path).expect("create storage");
        storage
            .set_pin_reason("rg", "pin for scripts".to_string())
            .expect("set reason");
        let storage = MetadataStorage::new(&path).expect("reload");
        assert_eq!(
            storage
                .get_package("rg")
                .and_then(|m| m.pin_reason.as_deref()),
            Some("pin for scripts")
        );

        let mut storage = MetadataStorage::new(&path).expect("reload mutable");
        storage.clear_pin_reason("rg").expect("clear reason");
        let storage = MetadataStorage::new(&path).expect("reload after clear");
        assert!(storage.get_package("rg").is_none());
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn rename_migrates_entry() {
        let path = temp_metadata_file("rename");
        let mut storage = MetadataStorage::new(&path).expect("create storage");
        storage
            .set_pin_reason("old", "why".to_string())
            .expect("set reason");
        storage.rename_package("old", "new").expect("rename");
        let storage = MetadataStorage::new(&path).expect("reload");
        assert!(storage.get_package("old").is_none());
        assert_eq!(
            storage
                .get_package("new")
                .and_then(|m| m.pin_reason.as_deref()),
            Some("why")
        );
        cleanup(&path).expect("cleanup");
    }
}
