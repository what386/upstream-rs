use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::utils::filesystem::atomic_ops::write_atomic;

pub const TRANSACTION_STORAGE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    Install,
    Build,
    Remove,
    Upgrade,
    Reinstall,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    Started,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionPackageStatus {
    Planned,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoActionKind {
    Remove,
    RestoreRollback,
    Reinstall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoAction {
    pub kind: UndoActionKind,
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionPackage {
    pub name: String,
    pub status: TransactionPackageStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TransactionPackage {
    pub fn planned(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: TransactionPackageStatus::Planned,
            old_version: None,
            new_version: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: String,
    pub kind: TransactionKind,
    pub status: TransactionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub packages: Vec<TransactionPackage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<UndoAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TransactionRecord {
    pub fn new(
        id: impl Into<String>,
        kind: TransactionKind,
        packages: Vec<TransactionPackage>,
        undo: Option<UndoAction>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            kind,
            status: TransactionStatus::Started,
            created_at: now,
            updated_at: now,
            packages,
            undo,
            error: None,
        }
    }

    pub fn is_reversible(&self) -> bool {
        self.status == TransactionStatus::Completed && self.undo.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransactionStorageFile {
    version: u32,
    transactions: Vec<TransactionRecord>,
}

impl Default for TransactionStorageFile {
    fn default() -> Self {
        Self {
            version: TRANSACTION_STORAGE_VERSION,
            transactions: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct TransactionStorage {
    file: TransactionStorageFile,
    transactions_file: PathBuf,
}

impl TransactionStorage {
    pub fn new(transactions_file: &Path) -> Result<Self> {
        let mut storage = Self {
            file: TransactionStorageFile::default(),
            transactions_file: transactions_file.to_path_buf(),
        };
        storage.load()?;
        Ok(storage)
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.transactions_file.exists() {
            self.file = TransactionStorageFile::default();
            return Ok(());
        }

        let json = fs::read_to_string(&self.transactions_file).with_context(|| {
            format!(
                "Failed to read transaction storage '{}'",
                self.transactions_file.display()
            )
        })?;

        if json.trim().is_empty() {
            self.file = TransactionStorageFile::default();
            return Ok(());
        }

        let parsed: TransactionStorageFile = serde_json::from_str(&json).with_context(|| {
            format!(
                "Failed to parse transaction storage '{}'",
                self.transactions_file.display()
            )
        })?;
        if parsed.version != TRANSACTION_STORAGE_VERSION {
            return Err(anyhow!(
                "Unsupported transaction storage version {} in '{}'. Expected version {}.",
                parsed.version,
                self.transactions_file.display(),
                TRANSACTION_STORAGE_VERSION
            ));
        }
        self.file = parsed;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.file)
            .context("Failed to serialize transaction storage")?;
        write_atomic(&self.transactions_file, json.as_bytes()).with_context(|| {
            format!(
                "Failed to write transaction storage to '{}'",
                self.transactions_file.display()
            )
        })
    }

    pub fn all(&self) -> &[TransactionRecord] {
        &self.file.transactions
    }

    pub fn recent(&self, limit: usize) -> Vec<&TransactionRecord> {
        self.file.transactions.iter().rev().take(limit).collect()
    }

    pub fn get(&self, id: &str) -> Option<&TransactionRecord> {
        self.file
            .transactions
            .iter()
            .find(|transaction| transaction.id == id)
    }

    pub fn latest_reversible(&self) -> Option<&TransactionRecord> {
        self.file
            .transactions
            .iter()
            .rev()
            .find(|transaction| transaction.is_reversible())
    }

    pub fn append(&mut self, transaction: TransactionRecord) -> Result<()> {
        if self.get(&transaction.id).is_some() {
            return Err(anyhow!("Transaction '{}' already exists", transaction.id));
        }

        self.file.transactions.push(transaction);
        self.save()
    }

    pub fn update<F>(&mut self, id: &str, update: F) -> Result<bool>
    where
        F: FnOnce(&mut TransactionRecord),
    {
        let Some(transaction) = self
            .file
            .transactions
            .iter_mut()
            .find(|transaction| transaction.id == id)
        else {
            return Ok(false);
        };

        update(transaction);
        transaction.updated_at = Utc::now();
        self.save()?;
        Ok(true)
    }

    pub fn mark_completed(&mut self, id: &str) -> Result<bool> {
        self.update(id, |transaction| {
            transaction.status = TransactionStatus::Completed;
            transaction.error = None;
        })
    }

    pub fn mark_failed(&mut self, id: &str, error: impl Into<String>) -> Result<bool> {
        let error = error.into();
        self.update(id, |transaction| {
            transaction.status = TransactionStatus::Failed;
            transaction.error = Some(error);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TRANSACTION_STORAGE_VERSION, TransactionKind, TransactionPackage, TransactionRecord,
        TransactionStatus, TransactionStorage, UndoAction, UndoActionKind,
    };
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_transactions_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-transaction-storage-test-{name}-{nanos}"))
            .join("transactions.json")
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    fn install_transaction(id: &str, package: &str) -> TransactionRecord {
        TransactionRecord::new(
            id,
            TransactionKind::Install,
            vec![TransactionPackage::planned(package)],
            Some(UndoAction {
                kind: UndoActionKind::Remove,
                packages: vec![package.to_string()],
            }),
        )
    }

    #[test]
    fn new_starts_empty_when_file_missing() {
        let path = temp_transactions_file("missing");
        let storage = TransactionStorage::new(&path).expect("create storage");
        assert!(storage.all().is_empty());
    }

    #[test]
    fn append_and_reload_round_trips_transaction() {
        let path = temp_transactions_file("roundtrip");
        let mut storage = TransactionStorage::new(&path).expect("create storage");
        storage
            .append(install_transaction("tx-1", "rg"))
            .expect("append");

        let reloaded = TransactionStorage::new(&path).expect("reload");
        let transaction = reloaded.get("tx-1").expect("transaction");
        assert_eq!(transaction.kind, TransactionKind::Install);
        assert_eq!(transaction.status, TransactionStatus::Started);
        assert_eq!(transaction.packages[0].name, "rg");
        assert_eq!(
            transaction.undo.as_ref().map(|undo| &undo.kind),
            Some(&UndoActionKind::Remove)
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn append_rejects_duplicate_ids() {
        let path = temp_transactions_file("duplicates");
        let mut storage = TransactionStorage::new(&path).expect("create storage");
        storage
            .append(install_transaction("tx-1", "rg"))
            .expect("append first");
        let err = storage
            .append(install_transaction("tx-1", "fd"))
            .expect_err("duplicate id should fail");

        assert!(err.to_string().contains("already exists"));
        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn mark_completed_and_latest_reversible_track_status() {
        let path = temp_transactions_file("complete");
        let mut storage = TransactionStorage::new(&path).expect("create storage");
        storage
            .append(install_transaction("tx-1", "rg"))
            .expect("append first");
        storage
            .append(TransactionRecord::new(
                "tx-2",
                TransactionKind::Remove,
                vec![TransactionPackage::planned("fd")],
                None,
            ))
            .expect("append second");

        assert!(storage.latest_reversible().is_none());
        assert!(storage.mark_completed("tx-1").expect("mark complete"));
        assert!(storage.mark_completed("tx-2").expect("mark complete"));

        assert_eq!(
            storage.latest_reversible().expect("latest reversible").id,
            "tx-1"
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn mark_failed_records_error_and_returns_false_for_missing_id() {
        let path = temp_transactions_file("failed");
        let mut storage = TransactionStorage::new(&path).expect("create storage");
        storage
            .append(install_transaction("tx-1", "rg"))
            .expect("append");

        assert!(
            storage
                .mark_failed("tx-1", "download failed")
                .expect("mark failed")
        );
        assert!(
            !storage
                .mark_failed("missing", "not found")
                .expect("missing id")
        );

        let transaction = storage.get("tx-1").expect("transaction");
        assert_eq!(transaction.status, TransactionStatus::Failed);
        assert_eq!(transaction.error.as_deref(), Some("download failed"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn recent_returns_newest_first_with_limit() {
        let path = temp_transactions_file("recent");
        let mut storage = TransactionStorage::new(&path).expect("create storage");
        storage
            .append(install_transaction("tx-1", "one"))
            .expect("append first");
        storage
            .append(install_transaction("tx-2", "two"))
            .expect("append second");
        storage
            .append(install_transaction("tx-3", "three"))
            .expect("append third");

        let ids = storage
            .recent(2)
            .into_iter()
            .map(|transaction| transaction.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["tx-3", "tx-2"]);

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn invalid_json_file_returns_parse_error() {
        let path = temp_transactions_file("invalid-json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, "{not-json").expect("write invalid json");

        let err = TransactionStorage::new(&path).expect_err("invalid json should fail");
        assert!(
            err.to_string()
                .contains("Failed to parse transaction storage")
        );

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn unsupported_version_returns_error() {
        let path = temp_transactions_file("unsupported-version");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "version": TRANSACTION_STORAGE_VERSION + 1,
                "transactions": []
            }))
            .expect("serialize"),
        )
        .expect("write unsupported version");

        let err = TransactionStorage::new(&path).expect_err("unsupported version should fail");
        assert!(
            err.to_string()
                .contains("Unsupported transaction storage version")
        );

        cleanup(&path).expect("cleanup");
    }
}
