use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::utils::filesystem::atomic_ops::write_atomic;

pub const MANIFEST_FILE_NAME: &str = "migration.json";
pub const MANIFEST_STORAGE_VERSION: u32 = 1;
pub const CURRENT_LAYOUT_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub family: String,
}

impl PlatformInfo {
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            family: std::env::consts::FAMILY.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationManifest {
    pub manifest_version: u32,
    pub layout_version: u32,
    pub root_id: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub last_migrated_by: String,
    pub last_migrated_at: DateTime<Utc>,
    pub previous_layout_version: Option<u32>,
    pub platform: PlatformInfo,
}

impl MigrationManifest {
    pub fn current() -> Self {
        let now = Utc::now();
        Self {
            manifest_version: MANIFEST_STORAGE_VERSION,
            layout_version: CURRENT_LAYOUT_VERSION,
            root_id: generate_root_id(now),
            created_by: current_upstream_version(),
            created_at: now,
            last_migrated_by: current_upstream_version(),
            last_migrated_at: now,
            previous_layout_version: None,
            platform: PlatformInfo::current(),
        }
    }

    pub fn record_migration(&mut self, layout_version: u32) {
        self.record_migration_from(None, layout_version);
    }

    pub fn record_migration_from(
        &mut self,
        previous_layout_version: Option<u32>,
        layout_version: u32,
    ) {
        let previous = previous_layout_version.unwrap_or(self.layout_version);
        self.layout_version = layout_version;
        self.last_migrated_by = current_upstream_version();
        self.last_migrated_at = Utc::now();
        self.previous_layout_version = (previous != layout_version).then_some(previous);
    }
}

#[derive(Debug)]
pub struct ManifestStorage {
    manifest_file: PathBuf,
    manifest: Option<MigrationManifest>,
}

impl ManifestStorage {
    pub fn new(migration_file: &Path) -> Result<Self> {
        let mut storage = Self {
            manifest_file: migration_file.to_path_buf(),
            manifest: None,
        };
        storage.load()?;
        Ok(storage)
    }

    pub fn path_for_root(upstream_root: &Path) -> PathBuf {
        upstream_root.join(MANIFEST_FILE_NAME)
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.manifest_file.exists() {
            self.manifest = None;
            return Ok(());
        }

        let json = fs::read_to_string(&self.manifest_file).with_context(|| {
            format!(
                "Failed to read migration manifest '{}'",
                self.manifest_file.display()
            )
        })?;
        if json.trim().is_empty() {
            self.manifest = None;
            return Ok(());
        }

        let manifest: MigrationManifest = serde_json::from_str(&json).with_context(|| {
            format!(
                "Failed to parse migration manifest '{}'",
                self.manifest_file.display()
            )
        })?;
        if manifest.manifest_version != MANIFEST_STORAGE_VERSION {
            return Err(anyhow!(
                "Unsupported migration manifest version {} in '{}'. Expected version {}.",
                manifest.manifest_version,
                self.manifest_file.display(),
                MANIFEST_STORAGE_VERSION
            ));
        }

        self.manifest = Some(manifest);
        Ok(())
    }

    pub fn manifest(&self) -> Option<&MigrationManifest> {
        self.manifest.as_ref()
    }

    pub fn save_manifest(&mut self, manifest: MigrationManifest) -> Result<()> {
        if manifest.manifest_version != MANIFEST_STORAGE_VERSION {
            return Err(anyhow!(
                "Cannot save unsupported migration manifest version {}. Expected version {}.",
                manifest.manifest_version,
                MANIFEST_STORAGE_VERSION
            ));
        }

        let json = serde_json::to_string_pretty(&manifest)
            .context("Failed to serialize migration manifest")?;
        write_atomic(&self.manifest_file, json.as_bytes()).with_context(|| {
            format!(
                "Failed to write migration manifest to '{}'",
                self.manifest_file.display()
            )
        })?;
        self.manifest = Some(manifest);
        Ok(())
    }

    pub fn ensure_current(&mut self) -> Result<()> {
        if self.manifest.is_none() {
            self.save_manifest(MigrationManifest::current())?;
        }
        Ok(())
    }

    pub fn record_migration(&mut self, layout_version: u32) -> Result<()> {
        self.record_migration_from(None, layout_version)
    }

    pub fn record_migration_from(
        &mut self,
        previous_layout_version: Option<u32>,
        layout_version: u32,
    ) -> Result<()> {
        let mut manifest = self
            .manifest
            .clone()
            .unwrap_or_else(MigrationManifest::current);
        manifest.record_migration_from(previous_layout_version, layout_version);
        self.save_manifest(manifest)
    }
}

fn current_upstream_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn generate_root_id(now: DateTime<Utc>) -> String {
    format!("upstream-{}-{}", now.timestamp_micros(), process::id())
}

#[cfg(test)]
mod tests {
    use super::{
        CURRENT_LAYOUT_VERSION, MANIFEST_STORAGE_VERSION, ManifestStorage, MigrationManifest,
        PlatformInfo,
    };
    use chrono::Utc;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_manifest_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-migration-manifest-test-{name}-{nanos}"))
            .join("migration.json")
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    fn manifest(layout_version: u32) -> MigrationManifest {
        let now = Utc::now();
        MigrationManifest {
            manifest_version: MANIFEST_STORAGE_VERSION,
            layout_version,
            root_id: "root".to_string(),
            created_by: "test".to_string(),
            created_at: now,
            last_migrated_by: "test".to_string(),
            last_migrated_at: now,
            previous_layout_version: None,
            platform: PlatformInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                family: "unix".to_string(),
            },
        }
    }

    #[test]
    fn new_starts_empty_when_manifest_is_missing() {
        let path = temp_manifest_file("missing");
        let storage = ManifestStorage::new(&path).expect("create storage");
        assert!(storage.manifest().is_none());
    }

    #[test]
    fn ensure_current_writes_migration_manifest() {
        let path = temp_manifest_file("ensure-current");
        let mut storage = ManifestStorage::new(&path).expect("create storage");
        storage.ensure_current().expect("ensure current");

        let reloaded = ManifestStorage::new(&path).expect("reload storage");
        let manifest = reloaded.manifest().expect("manifest exists");
        assert_eq!(manifest.manifest_version, MANIFEST_STORAGE_VERSION);
        assert_eq!(manifest.layout_version, CURRENT_LAYOUT_VERSION);
        assert!(!manifest.root_id.is_empty());
        assert_eq!(manifest.platform.os, std::env::consts::OS);
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn record_migration_preserves_creation_fields_and_tracks_previous_layout() {
        let path = temp_manifest_file("record");
        let mut storage = ManifestStorage::new(&path).expect("create storage");
        storage
            .save_manifest(manifest(1))
            .expect("save old manifest");
        let created_at = storage.manifest().expect("manifest").created_at;

        storage
            .record_migration(CURRENT_LAYOUT_VERSION)
            .expect("record migration");

        let manifest = storage.manifest().expect("manifest");
        assert_eq!(manifest.layout_version, CURRENT_LAYOUT_VERSION);
        assert_eq!(manifest.previous_layout_version, Some(1));
        assert_eq!(manifest.created_at, created_at);
        assert_eq!(manifest.root_id, "root");
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn rejects_unsupported_manifest_version() {
        let path = temp_manifest_file("bad-version");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(
            &path,
            r#"{"manifest_version":2,"layout_version":1,"root_id":"root","created_by":"test","created_at":"2026-01-01T00:00:00Z","last_migrated_by":"test","last_migrated_at":"2026-01-01T00:00:00Z","previous_layout_version":null,"platform":{"os":"linux","arch":"x86_64","family":"unix"}}"#,
        )
        .expect("write manifest");

        let err = ManifestStorage::new(&path).expect_err("unsupported version");
        assert!(
            err.to_string()
                .contains("Unsupported migration manifest version")
        );
        cleanup(&path).expect("cleanup");
    }
}
