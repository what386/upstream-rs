use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::services::trust::{CosignPublicKey, MinisignPublicKey, TrustedSignatureKeys};
use crate::utils::filesystem::atomic_ops::write_atomic;

pub const TRUST_STORAGE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustStorageFile {
    version: u32,
    minisign_public_keys: Vec<MinisignPublicKey>,
    cosign_public_keys: Vec<CosignPublicKey>,
}

impl Default for TrustStorageFile {
    fn default() -> Self {
        Self {
            version: TRUST_STORAGE_VERSION,
            minisign_public_keys: Vec::new(),
            cosign_public_keys: Vec::new(),
        }
    }
}

pub struct KeyMergeSummary {
    pub imported: usize,
    pub deduped: usize,
    pub total: usize,
}

pub struct SignatureKeyMergeSummary {
    pub minisign: KeyMergeSummary,
    pub cosign: KeyMergeSummary,
}

#[derive(Debug)]
pub struct TrustStorage {
    file: TrustStorageFile,
    trust_file: PathBuf,
}

impl TrustStorage {
    pub fn new(trust_file: &Path) -> Result<Self> {
        let mut storage = Self {
            file: TrustStorageFile::default(),
            trust_file: trust_file.to_path_buf(),
        };
        storage.load()?;
        Ok(storage)
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.trust_file.exists() {
            self.file = TrustStorageFile::default();
            return Ok(());
        }

        match fs::read_to_string(&self.trust_file) {
            Ok(json) => {
                if json.trim().is_empty() {
                    self.file = TrustStorageFile::default();
                    return Ok(());
                }
                let file: TrustStorageFile = serde_json::from_str(&json).with_context(|| {
                    format!(
                        "Failed to parse trust storage '{}'",
                        self.trust_file.display()
                    )
                })?;
                if file.version != TRUST_STORAGE_VERSION {
                    return Err(anyhow!(
                        "Unsupported trust storage version {} in '{}'. Expected version {}.",
                        file.version,
                        self.trust_file.display(),
                        TRUST_STORAGE_VERSION
                    ));
                }
                self.file = file;
                Ok(())
            }
            Err(e) => Err(anyhow!("Warning: Failed to load trust storage: {}", e)),
        }
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.file)
            .context("Failed to serialize trust storage")?;
        write_atomic(&self.trust_file, json.as_bytes()).with_context(|| {
            format!(
                "Failed to write trust storage to '{}'",
                self.trust_file.display()
            )
        })
    }

    pub fn ensure_exists(&self) -> Result<()> {
        if !self.trust_file.exists() {
            self.save()?;
        }
        Ok(())
    }

    pub fn trusted_signature_keys(&self) -> TrustedSignatureKeys {
        TrustedSignatureKeys {
            minisign_public_keys: self.file.minisign_public_keys.clone(),
            cosign_public_keys: self.file.cosign_public_keys.clone(),
        }
    }

    pub fn merge_trusted_minisign_keys(
        &mut self,
        keys: &[MinisignPublicKey],
    ) -> Result<KeyMergeSummary> {
        let mut imported = 0_usize;
        let mut deduped = 0_usize;

        for key in keys {
            let normalized = key.key.trim();
            if normalized.is_empty() {
                continue;
            }
            let duplicate = self
                .file
                .minisign_public_keys
                .iter()
                .any(|existing| existing.key.trim().eq_ignore_ascii_case(normalized));
            if duplicate {
                deduped += 1;
                continue;
            }

            self.file.minisign_public_keys.push(MinisignPublicKey {
                id: key.id.clone(),
                key: normalized.to_string(),
            });
            imported += 1;
        }

        self.save()?;

        Ok(KeyMergeSummary {
            imported,
            deduped,
            total: self.file.minisign_public_keys.len(),
        })
    }

    pub fn merge_trusted_cosign_keys(
        &mut self,
        keys: &[CosignPublicKey],
    ) -> Result<KeyMergeSummary> {
        let mut imported = 0_usize;
        let mut deduped = 0_usize;

        for key in keys {
            let normalized = key.key.trim();
            if normalized.is_empty() {
                continue;
            }
            let duplicate = self
                .file
                .cosign_public_keys
                .iter()
                .any(|existing| existing.key.trim() == normalized);
            if duplicate {
                deduped += 1;
                continue;
            }

            self.file.cosign_public_keys.push(CosignPublicKey {
                id: key.id.clone(),
                key: normalized.to_string(),
            });
            imported += 1;
        }

        self.save()?;

        Ok(KeyMergeSummary {
            imported,
            deduped,
            total: self.file.cosign_public_keys.len(),
        })
    }

    pub fn merge_trusted_keys(
        &mut self,
        minisign_keys: &[MinisignPublicKey],
        cosign_keys: &[CosignPublicKey],
    ) -> Result<SignatureKeyMergeSummary> {
        let minisign = self.merge_trusted_minisign_keys(minisign_keys)?;
        let cosign = self.merge_trusted_cosign_keys(cosign_keys)?;
        Ok(SignatureKeyMergeSummary { minisign, cosign })
    }
}

#[cfg(test)]
mod tests {
    use super::{TRUST_STORAGE_VERSION, TrustStorage};
    use crate::services::trust::{CosignPublicKey, MinisignPublicKey};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_trust_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-trust-storage-test-{name}-{nanos}"))
            .join("trust.json")
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn new_starts_empty_when_file_missing() {
        let path = temp_trust_file("missing");
        let storage = TrustStorage::new(&path).expect("create storage");
        let keys = storage.trusted_signature_keys();
        assert!(keys.minisign_public_keys.is_empty());
        assert!(keys.cosign_public_keys.is_empty());
    }

    #[test]
    fn merge_keys_dedupes_and_round_trips() {
        let path = temp_trust_file("merge");
        let mut storage = TrustStorage::new(&path).expect("create storage");
        let summary = storage
            .merge_trusted_keys(
                &[
                    MinisignPublicKey {
                        id: Some("one".to_string()),
                        key: "RWabc".to_string(),
                    },
                    MinisignPublicKey {
                        id: Some("dupe".to_string()),
                        key: "rwABC".to_string(),
                    },
                ],
                &[CosignPublicKey {
                    id: None,
                    key: "-----BEGIN PUBLIC KEY-----\nkey\n-----END PUBLIC KEY-----".to_string(),
                }],
            )
            .expect("merge keys");

        assert_eq!(summary.minisign.imported, 1);
        assert_eq!(summary.minisign.deduped, 1);
        assert_eq!(summary.cosign.imported, 1);

        let storage = TrustStorage::new(&path).expect("reload");
        let keys = storage.trusted_signature_keys();
        assert_eq!(keys.minisign_public_keys.len(), 1);
        assert_eq!(keys.cosign_public_keys.len(), 1);
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn ensure_exists_writes_empty_trust_storage() {
        let path = temp_trust_file("ensure");
        let storage = TrustStorage::new(&path).expect("create storage");
        storage.ensure_exists().expect("ensure exists");

        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read trust file"))
                .expect("parse trust file");
        assert_eq!(
            value["version"].as_u64(),
            Some(TRUST_STORAGE_VERSION as u64)
        );
        assert_eq!(
            value["minisign_public_keys"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(
            value["cosign_public_keys"].as_array().map(Vec::len),
            Some(0)
        );
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn rejects_unsupported_version() {
        let path = temp_trust_file("bad-version");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(
            &path,
            r#"{"version":2,"minisign_public_keys":[],"cosign_public_keys":[]}"#,
        )
        .expect("write trust file");

        let err = TrustStorage::new(&path).expect_err("unsupported version");
        assert!(
            err.to_string()
                .contains("Unsupported trust storage version")
        );
        cleanup(&path).expect("cleanup");
    }
}
