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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RollbackArtifactFormat {
    #[default]
    Raw,
    Tgz,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRecord {
    pub package_snapshot: Package,
    pub artifact_relative_path: PathBuf,
    #[serde(default)]
    pub icon_relative_path: Option<PathBuf>,
    #[serde(default)]
    pub artifact_format: RollbackArtifactFormat,
    #[serde(default)]
    pub artifact_entry_path: Option<PathBuf>,
    #[serde(default)]
    pub icon_entry_path: Option<PathBuf>,
    pub source: RollbackSource,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RollbackStorageFile {
    version: u32,
    records: HashMap<String, Vec<RollbackRecord>>,
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
        self.file
            .records
            .get(package_name)
            .and_then(|records| records.last())
    }

    pub fn get_records(&self, package_name: &str) -> &[RollbackRecord] {
        self.file
            .records
            .get(package_name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn list_records(&self) -> &HashMap<String, Vec<RollbackRecord>> {
        &self.file.records
    }

    pub fn upsert_record(&mut self, package_name: &str, record: RollbackRecord) -> Result<()> {
        self.push_record(package_name, record, 1).map(|_| ())
    }

    pub fn push_record(
        &mut self,
        package_name: &str,
        record: RollbackRecord,
        max_records: usize,
    ) -> Result<Vec<RollbackRecord>> {
        let records = self
            .file
            .records
            .entry(package_name.to_string())
            .or_default();
        records.push(record);
        let pruned = if max_records > 0 && records.len() > max_records {
            let remove_count = records.len() - max_records;
            records.drain(0..remove_count).collect()
        } else {
            Vec::new()
        };
        self.save()?;
        Ok(pruned)
    }

    pub fn remove_record(&mut self, package_name: &str) -> Result<Option<RollbackRecord>> {
        let removed = self.file.records.get_mut(package_name).and_then(Vec::pop);
        if self
            .file
            .records
            .get(package_name)
            .is_some_and(Vec::is_empty)
        {
            self.file.records.remove(package_name);
        }
        self.save()?;
        Ok(removed)
    }

    pub fn remove_all_records(&mut self, package_name: &str) -> Result<Vec<RollbackRecord>> {
        let removed = self.file.records.remove(package_name).unwrap_or_default();
        self.save()?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::{RollbackArtifactFormat, RollbackRecord, RollbackSource, RollbackStorage};
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

    fn test_record(name: &str, source: RollbackSource) -> RollbackRecord {
        RollbackRecord {
            package_snapshot: test_package(name),
            artifact_relative_path: PathBuf::from(format!("{name}/{name}.old")),
            icon_relative_path: None,
            artifact_format: RollbackArtifactFormat::Raw,
            artifact_entry_path: None,
            icon_entry_path: None,
            source,
            created_at: Utc::now(),
        }
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
        let mut record = test_record("tool", RollbackSource::Upgrade);
        record.icon_relative_path = Some(PathBuf::from("tool/icon.png"));
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
            .upsert_record("tool", test_record("tool", RollbackSource::Remove))
            .expect("upsert");

        let removed = storage.remove_record("tool").expect("remove");
        assert!(removed.is_some());
        assert!(storage.get_record("tool").is_none());

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn push_record_keeps_latest_records_with_limit() {
        let path = temp_rollback_file("multiple");
        let mut storage = RollbackStorage::new(&path).expect("create storage");
        storage
            .push_record("tool", test_record("tool", RollbackSource::Upgrade), 2)
            .expect("push first");
        storage
            .push_record("tool", test_record("tool", RollbackSource::Remove), 2)
            .expect("push second");
        storage
            .push_record("tool", test_record("tool", RollbackSource::Reinstall), 2)
            .expect("push third");

        let records = storage.get_records("tool");
        assert_eq!(records.len(), 2);
        assert!(matches!(records[0].source, RollbackSource::Remove));
        assert!(matches!(records[1].source, RollbackSource::Reinstall));
        assert!(matches!(
            storage.get_record("tool").expect("latest").source,
            RollbackSource::Reinstall
        ));

        cleanup(&path).expect("cleanup");
    }
}
