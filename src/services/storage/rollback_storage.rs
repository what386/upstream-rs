use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::upstream::Package;
use crate::utils::filesystem::atomic_ops::write_atomic;

const ROLLBACK_STORAGE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackSource {
    Upgrade,
    Reinstall,
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRecord {
    pub package_snapshot: Package,
    pub artifact_relative_path: PathBuf,
    #[serde(default)]
    pub icon_relative_path: Option<PathBuf>,
    pub source: RollbackSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RollbackStorageFile {
    version: u32,
    records: HashMap<String, RollbackRecord>,
}

impl Default for RollbackStorageFile {
    fn default() -> Self {
        Self {
            version: ROLLBACK_STORAGE_VERSION,
            records: HashMap::new(),
        }
    }
}

pub struct RollbackStorage {
    file: RollbackStorageFile,
    rollback_file: PathBuf,
}

impl RollbackStorage {
    pub fn new(rollback_file: &Path) -> Result<Self> {
        let mut storage = Self {
            file: RollbackStorageFile::default(),
            rollback_file: rollback_file.to_path_buf(),
        };
        storage.load()?;
        Ok(storage)
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.rollback_file.exists() {
            self.file = RollbackStorageFile::default();
            return Ok(());
        }

        let json = fs::read_to_string(&self.rollback_file).with_context(|| {
            format!(
                "Failed to read rollback storage '{}'",
                self.rollback_file.display()
            )
        })?;

        if json.trim().is_empty() {
            self.file = RollbackStorageFile::default();
            return Ok(());
        }

        let parsed: RollbackStorageFile = serde_json::from_str(&json).with_context(|| {
            format!(
                "Failed to parse rollback storage '{}'",
                self.rollback_file.display()
            )
        })?;
        if parsed.version != ROLLBACK_STORAGE_VERSION {
            return Err(anyhow!(
                "Unsupported rollback storage version {} in '{}'. Expected version {}.",
                parsed.version,
                self.rollback_file.display(),
                ROLLBACK_STORAGE_VERSION
            ));
        }
        self.file = parsed;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.file)
            .context("Failed to serialize rollback storage")?;
        write_atomic(&self.rollback_file, json.as_bytes()).with_context(|| {
            format!(
                "Failed to write rollback storage to '{}'",
                self.rollback_file.display()
            )
        })
    }

    pub fn get_record(&self, package_name: &str) -> Option<&RollbackRecord> {
        self.file.records.get(package_name)
    }

    pub fn list_records(&self) -> &HashMap<String, RollbackRecord> {
        &self.file.records
    }

    pub fn upsert_record(&mut self, package_name: &str, record: RollbackRecord) -> Result<()> {
        self.file.records.insert(package_name.to_string(), record);
        self.save()
    }

    pub fn remove_record(&mut self, package_name: &str) -> Result<Option<RollbackRecord>> {
        let removed = self.file.records.remove(package_name);
        self.save()?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::{RollbackRecord, RollbackSource, RollbackStorage};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use chrono::Utc;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_rollback_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-rollback-storage-test-{name}-{nanos}"))
            .join("rollback.json")
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
    fn upsert_and_reload_record_round_trips() {
        let path = temp_rollback_file("roundtrip");
        let mut storage = RollbackStorage::new(&path).expect("create storage");
        let record = RollbackRecord {
            package_snapshot: test_package("tool"),
            artifact_relative_path: PathBuf::from("tool/tool.old"),
            icon_relative_path: Some(PathBuf::from("tool/icon.png")),
            source: RollbackSource::Upgrade,
            created_at: Utc::now(),
        };
        storage
            .upsert_record("tool", record.clone())
            .expect("upsert");

        let reloaded = RollbackStorage::new(&path).expect("reload");
        let loaded = reloaded.get_record("tool").expect("record");
        assert_eq!(loaded.package_snapshot.name, "tool");
        assert_eq!(loaded.artifact_relative_path, record.artifact_relative_path);
        assert!(loaded.icon_relative_path.is_some());

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn remove_record_returns_removed_value() {
        let path = temp_rollback_file("remove");
        let mut storage = RollbackStorage::new(&path).expect("create storage");
        storage
            .upsert_record(
                "tool",
                RollbackRecord {
                    package_snapshot: test_package("tool"),
                    artifact_relative_path: PathBuf::from("tool/tool.old"),
                    icon_relative_path: None,
                    source: RollbackSource::Remove,
                    created_at: Utc::now(),
                },
            )
            .expect("upsert");

        let removed = storage.remove_record("tool").expect("remove");
        assert!(removed.is_some());
        assert!(storage.get_record("tool").is_none());

        cleanup(&path).expect("cleanup");
    }
}
