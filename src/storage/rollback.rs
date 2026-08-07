use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::upstream::Package;
use crate::utils::filesystem::atomic_ops::write_atomic;

const ROLLBACK_STORAGE_VERSION: u32 = 1;

fn rollback_storage_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

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
        for (package_name, records) in &parsed.records {
            for record in records {
                validate_rollback_record(package_name, record)?;
            }
        }
        self.file = parsed;
        Ok(())
    }

    fn save(&self) -> Result<()> {
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
        let _guard = rollback_storage_lock()
            .lock()
            .map_err(|_| anyhow!("Rollback storage lock is poisoned"))?;
        self.load()?;
        validate_rollback_record(package_name, &record)?;

        let original_file = self.file.clone();
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

        if let Err(error) = self.save() {
            self.file = original_file;
            return Err(error);
        }
        Ok(pruned)
    }

    pub fn remove_record(&mut self, package_name: &str) -> Result<Option<RollbackRecord>> {
        let _guard = rollback_storage_lock()
            .lock()
            .map_err(|_| anyhow!("Rollback storage lock is poisoned"))?;
        self.load()?;

        let original_file = self.file.clone();
        let removed = self.file.records.get_mut(package_name).and_then(Vec::pop);
        if self
            .file
            .records
            .get(package_name)
            .is_some_and(Vec::is_empty)
        {
            self.file.records.remove(package_name);
        }
        if let Err(error) = self.save() {
            self.file = original_file;
            return Err(error);
        }
        Ok(removed)
    }

    pub fn remove_all_records(&mut self, package_name: &str) -> Result<Vec<RollbackRecord>> {
        let _guard = rollback_storage_lock()
            .lock()
            .map_err(|_| anyhow!("Rollback storage lock is poisoned"))?;
        self.load()?;

        let original_file = self.file.clone();
        let removed = self.file.records.remove(package_name).unwrap_or_default();
        if let Err(error) = self.save() {
            self.file = original_file;
            return Err(error);
        }
        Ok(removed)
    }

    pub fn rename_package(&mut self, old_name: &str, new_name: &str) -> Result<bool> {
        let _guard = rollback_storage_lock()
            .lock()
            .map_err(|_| anyhow!("Rollback storage lock is poisoned"))?;
        self.load()?;

        if self.file.records.contains_key(new_name) {
            return Err(anyhow!(
                "Rollback data already exists for package '{}'",
                new_name
            ));
        }
        let Some(original_records) = self.file.records.get(old_name).cloned() else {
            return Ok(false);
        };

        let mut renamed_records = original_records.clone();
        for record in &mut renamed_records {
            record.package_snapshot.name = new_name.to_string();
            record.artifact_relative_path =
                rebase_package_path(&record.artifact_relative_path, old_name, new_name)?;
            record.icon_relative_path = record
                .icon_relative_path
                .as_deref()
                .map(|path| rebase_package_path(path, old_name, new_name))
                .transpose()?;
        }
        self.file.records.remove(old_name);
        self.file
            .records
            .insert(new_name.to_string(), renamed_records);

        if let Err(error) = self.save() {
            self.file.records.remove(new_name);
            self.file
                .records
                .insert(old_name.to_string(), original_records);
            return Err(error).context(format!(
                "Failed to persist rollback rename from '{}' to '{}'",
                old_name, new_name
            ));
        }

        Ok(true)
    }
}

fn rebase_package_path(path: &Path, old_name: &str, new_name: &str) -> Result<PathBuf> {
    validate_package_relative_path(path, old_name)?;
    let mut components = path.components();
    let _ = components.next();
    let mut rebased = PathBuf::from(new_name);
    rebased.extend(components);
    Ok(rebased)
}

fn validate_rollback_record(package_name: &str, record: &RollbackRecord) -> Result<()> {
    if record.package_snapshot.name != package_name {
        return Err(anyhow!(
            "Rollback record key '{}' does not match snapshot package '{}'",
            package_name,
            record.package_snapshot.name
        ));
    }
    validate_package_relative_path(&record.artifact_relative_path, package_name)?;
    if let Some(icon_path) = &record.icon_relative_path {
        validate_package_relative_path(icon_path, package_name)?;
    }
    match record.artifact_format {
        RollbackArtifactFormat::Raw => {
            if record.artifact_entry_path.is_some() || record.icon_entry_path.is_some() {
                return Err(anyhow!(
                    "Raw rollback record for '{}' contains archive entry paths",
                    package_name
                ));
            }
        }
        RollbackArtifactFormat::Tgz => {
            let artifact_entry = record.artifact_entry_path.as_deref().ok_or_else(|| {
                anyhow!(
                    "Compressed rollback record for '{}' is missing its artifact entry path",
                    package_name
                )
            })?;
            validate_entry_path(artifact_entry, "artifact")?;
            if let Some(icon_entry) = record.icon_entry_path.as_deref() {
                validate_entry_path(icon_entry, "icon")?;
            }
        }
    }
    Ok(())
}

fn validate_package_relative_path(path: &Path, package_name: &str) -> Result<()> {
    let mut components = path.components();
    let Some(std::path::Component::Normal(first)) = components.next() else {
        return Err(anyhow!(
            "Rollback path '{}' is not a safe relative path",
            path.display()
        ));
    };
    if first != std::ffi::OsStr::new(package_name)
        || components.any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(anyhow!(
            "Rollback path '{}' is not safely stored under package '{}'",
            path.display(),
            package_name
        ));
    }
    Ok(())
}

fn validate_entry_path(path: &Path, expected_root: &str) -> Result<()> {
    let mut components = path.components();
    let Some(std::path::Component::Normal(first)) = components.next() else {
        return Err(anyhow!(
            "Rollback entry path '{}' is not a safe relative path",
            path.display()
        ));
    };
    if first != std::ffi::OsStr::new(expected_root)
        || components.any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(anyhow!(
            "Rollback entry path '{}' is not safely stored under '{}'",
            path.display(),
            expected_root
        ));
    }
    Ok(())
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
        if let Some(parent) = path.parent()
            && parent.exists()
        {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn upsert_and_reload_record_round_trips() {
        let path = temp_rollback_file("roundtrip");
        let mut storage = RollbackStorage::new(&path).expect("create storage");
        let mut record = test_record("tool", RollbackSource::Upgrade);
        record.package_snapshot.release_tag = Some("opaque-release".to_string());
        record.package_snapshot.release_published_at = Some(Utc::now());
        record.icon_relative_path = Some(PathBuf::from("tool/icon.png"));
        storage
            .upsert_record("tool", record.clone())
            .expect("upsert");

        let reloaded = RollbackStorage::new(&path).expect("reload");
        let loaded = reloaded.get_record("tool").expect("record");
        assert_eq!(loaded.package_snapshot.name, "tool");
        assert_eq!(
            loaded.package_snapshot.release_tag.as_deref(),
            Some("opaque-release")
        );
        assert_eq!(
            loaded.package_snapshot.release_published_at,
            record.package_snapshot.release_published_at
        );
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

    #[test]
    fn rename_package_rekeys_records_and_rebases_artifact_paths() {
        let path = temp_rollback_file("rename");
        let mut storage = RollbackStorage::new(&path).expect("create storage");
        let mut record = test_record("old", RollbackSource::Upgrade);
        record.icon_relative_path = Some(PathBuf::from("old/capture/icon.png"));
        storage
            .upsert_record("old", record)
            .expect("store old record");

        assert!(
            storage
                .rename_package("old", "new")
                .expect("rename rollback metadata")
        );
        assert!(storage.get_record("old").is_none());
        let renamed = storage.get_record("new").expect("renamed record");
        assert_eq!(renamed.package_snapshot.name, "new");
        assert_eq!(renamed.artifact_relative_path, PathBuf::from("new/old.old"));
        assert_eq!(
            renamed.icon_relative_path.as_deref(),
            Some(Path::new("new/capture/icon.png"))
        );

        let reloaded = RollbackStorage::new(&path).expect("reload renamed storage");
        assert_eq!(
            reloaded
                .get_record("new")
                .expect("persisted renamed record")
                .package_snapshot
                .name,
            "new"
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn concurrent_storage_instances_do_not_lose_records() {
        let path = temp_rollback_file("concurrent");
        let handles = (0..8)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let name = format!("tool-{index}");
                    let mut storage = RollbackStorage::new(&path).expect("open storage");
                    storage
                        .upsert_record(&name, test_record(&name, RollbackSource::Upgrade))
                        .expect("store record");
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().expect("join writer");
        }

        let storage = RollbackStorage::new(&path).expect("reload");
        for index in 0..8 {
            assert!(storage.get_record(&format!("tool-{index}")).is_some());
        }

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn rejects_rollback_paths_outside_the_package_directory() {
        let path = temp_rollback_file("unsafe-path");
        let mut storage = RollbackStorage::new(&path).expect("create storage");
        let mut record = test_record("tool", RollbackSource::Upgrade);
        record.artifact_relative_path = PathBuf::from("../outside");

        let error = storage
            .upsert_record("tool", record)
            .expect_err("unsafe rollback path should be rejected");

        assert!(error.to_string().contains("not a safe"));
        assert!(storage.get_record("tool").is_none());
        cleanup(&path).expect("cleanup");
    }
}
