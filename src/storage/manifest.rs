use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::utils::filesystem::atomic_ops::write_atomic;

pub const MANIFEST_FILE_NAME: &str = "migration.json";
pub const MANIFEST_STORAGE_VERSION: u32 = 2;
pub const CURRENT_LAYOUT_VERSION: u32 = 5;

/// The latest successfully completed layout version.
/// Database schema versions are tracked independently by SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationManifest {
    pub manifest_version: u32,
    pub layout_version: u32,
}

impl MigrationManifest {
    pub fn current() -> Self {
        Self {
            manifest_version: MANIFEST_STORAGE_VERSION,
            layout_version: CURRENT_LAYOUT_VERSION,
        }
    }
}

#[derive(Debug)]
pub struct ManifestStorage {
    manifest_file: PathBuf,
    manifest: Option<MigrationManifest>,
}

impl ManifestStorage {
    pub fn new(manifest_file: &Path) -> Result<Self> {
        let manifest = match fs::read_to_string(manifest_file) {
            Ok(json) => {
                let value: serde_json::Value = serde_json::from_str(&json).with_context(|| {
                    format!(
                        "Failed to parse migration ledger '{}'",
                        manifest_file.display()
                    )
                })?;
                match value
                    .get("manifest_version")
                    .and_then(serde_json::Value::as_u64)
                {
                    Some(1) => {
                        // Old manifests could mark a layout current before all steps finished.
                        // Validate their files once before replacing them with the new ledger.
                        let version = value
                            .get("layout_version")
                            .and_then(serde_json::Value::as_u64)
                            .context("Legacy migration manifest has no layout version")?;
                        validate_layout(version)?;
                        None
                    }
                    Some(2) => {
                        let manifest: MigrationManifest = serde_json::from_value(value)?;
                        validate_layout(u64::from(manifest.layout_version))?;
                        Some(manifest)
                    }
                    version => bail!(
                        "Unsupported migration manifest version {version:?} in '{}'",
                        manifest_file.display()
                    ),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Failed to read migration ledger '{}'",
                        manifest_file.display()
                    )
                });
            }
        };
        Ok(Self {
            manifest_file: manifest_file.to_owned(),
            manifest,
        })
    }

    pub fn path_for_root(root: &Path) -> PathBuf {
        root.join(MANIFEST_FILE_NAME)
    }

    pub fn manifest(&self) -> Option<&MigrationManifest> {
        self.manifest.as_ref()
    }

    pub fn save_manifest(&mut self, manifest: MigrationManifest) -> Result<()> {
        if manifest.manifest_version != MANIFEST_STORAGE_VERSION {
            bail!(
                "Unsupported migration manifest version {}",
                manifest.manifest_version
            );
        }
        validate_layout(u64::from(manifest.layout_version))?;
        let json = serde_json::to_vec_pretty(&manifest)?;
        write_atomic(&self.manifest_file, &json)?;
        self.manifest = Some(manifest);
        Ok(())
    }

    /// Call after compatibility checks and required work have succeeded.
    pub fn ensure_current(&mut self) -> Result<()> {
        if self.manifest.is_none() {
            self.save_manifest(MigrationManifest::current())?;
        }
        Ok(())
    }
}

fn validate_layout(version: u64) -> Result<()> {
    if version < u64::from(CURRENT_LAYOUT_VERSION) {
        bail!(
            "Installation layout {version} is no longer supported. Upgrade this installation with Upstream v2.24.1 before using this version."
        );
    }
    if version > u64::from(CURRENT_LAYOUT_VERSION) {
        bail!("Installation layout {version} is newer than this version of Upstream supports");
    }
    Ok(())
}
