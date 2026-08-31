mod rb_artifacts;
mod rb_index;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::models::common::enums::CompressionLevel;
use crate::models::upstream::Package;

pub use rb_index::{RollbackArtifactFormat, RollbackRecord, RollbackSource};

use rb_artifacts::RollbackArtifacts;
pub(crate) use rb_artifacts::{ArtifactSizeEstimate, PreparedRollback};
use rb_index::RollbackIndex;

pub(crate) struct RollbackCapture {
    pub(crate) artifact_path: PathBuf,
    pub(crate) cleanup_warnings: Vec<String>,
}

/// Front-facing coordinator for rollback metadata and on-disk artifacts.
///
/// Mutations are exposed only as coordinated operations so index and artifact
/// changes are compensated together when one side fails.
pub(crate) struct RollbackStorage {
    index: RollbackIndex,
    artifacts_dir: PathBuf,
}

impl RollbackStorage {
    pub(crate) fn new(rollback_file: &Path, artifacts_dir: &Path) -> Result<Self> {
        Ok(Self {
            index: RollbackIndex::new(rollback_file)?,
            artifacts_dir: artifacts_dir.to_path_buf(),
        })
    }

    pub(crate) fn get_record(&self, package_name: &str) -> Option<&RollbackRecord> {
        self.index.get_record(package_name)
    }

    #[cfg(test)]
    pub(crate) fn get_records(&self, package_name: &str) -> &[RollbackRecord] {
        self.index.get_records(package_name)
    }

    pub(crate) fn records_pruned_by_next_capture(
        &self,
        package_name: &str,
        max_records: usize,
    ) -> &[RollbackRecord] {
        self.index
            .records_pruned_by_next_push(package_name, max_records)
    }

    pub(crate) fn list_records(&self) -> &HashMap<String, Vec<RollbackRecord>> {
        self.index.list_records()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn capture_artifact(
        &mut self,
        package: &Package,
        artifact_path: &Path,
        icon_source: Option<&Path>,
        source: RollbackSource,
        compression_level: CompressionLevel,
        max_records: usize,
    ) -> Result<RollbackCapture> {
        let artifacts = RollbackArtifacts::new(&self.artifacts_dir);
        let created = artifacts.create(
            package,
            artifact_path,
            icon_source,
            source,
            compression_level,
        )?;

        let pruned = match self
            .index
            .push_record(&package.id, created.record.clone(), max_records)
        {
            Ok(pruned) => pruned,
            Err(index_error) => {
                return match artifacts.delete(&package.id, &created.record) {
                    Ok(()) => Err(index_error).context("Failed to persist rollback record"),
                    Err(cleanup_error) => Err(anyhow!(
                        "Failed to persist rollback record: {index_error:#}. Failed to clean unrecorded rollback artifact: {cleanup_error:#}"
                    )),
                };
            }
        };

        let mut cleanup_warnings = Vec::new();
        for record in pruned {
            if let Err(error) = artifacts.delete(&package.id, &record) {
                cleanup_warnings.push(format!(
                    "failed to delete pruned rollback artifact for '{}': {error:#}",
                    package.id
                ));
            }
        }

        Ok(RollbackCapture {
            artifact_path: created.artifact_path,
            cleanup_warnings,
        })
    }

    pub(crate) fn prepare_record(
        &self,
        package_name: &str,
        record: &RollbackRecord,
    ) -> Result<PreparedRollback> {
        RollbackArtifacts::new(&self.artifacts_dir).prepare(package_name, record)
    }

    pub(crate) fn consume_record(
        &mut self,
        package_name: &str,
        record: &RollbackRecord,
    ) -> Result<()> {
        self.index.remove_record(package_name)?;
        RollbackArtifacts::new(&self.artifacts_dir).delete(package_name, record)
    }

    pub(crate) fn prune_package(&mut self, package_name: &str) -> Result<bool> {
        let removed = self.index.remove_all_records(package_name)?;
        let artifacts = RollbackArtifacts::new(&self.artifacts_dir);

        for record in &removed {
            artifacts.delete(package_name, record)?;
        }

        artifacts.cleanup_empty_package_dir(package_name)?;
        Ok(!removed.is_empty())
    }

    pub(crate) fn rename_package(&mut self, old_name: &str, new_name: &str) -> Result<bool> {
        let artifacts = RollbackArtifacts::new(&self.artifacts_dir);
        let moved_directory = artifacts.rename_package_dir(old_name, new_name)?;

        match self.index.rename_package(old_name, new_name) {
            Ok(renamed_metadata) => Ok(moved_directory || renamed_metadata),
            Err(index_error) => {
                if moved_directory
                    && let Err(rollback_error) = artifacts.rename_package_dir(new_name, old_name)
                {
                    return Err(anyhow!(
                        "Failed to rename rollback metadata from '{}' to '{}': {}. Directory rollback also failed: {}",
                        old_name,
                        new_name,
                        index_error,
                        rollback_error
                    ));
                }

                Err(index_error)
            }
        }
    }

    pub(crate) fn rollback_record_size(&self, record: &RollbackRecord) -> Result<u64> {
        RollbackArtifacts::new(&self.artifacts_dir).rollback_record_size(record)
    }

    pub(crate) fn package_artifacts_size(&self, package_name: &str) -> Result<u64> {
        RollbackArtifacts::new(&self.artifacts_dir).package_size(package_name)
    }

    pub(crate) fn restored_size(&self, record: &RollbackRecord) -> Result<ArtifactSizeEstimate> {
        RollbackArtifacts::new(&self.artifacts_dir).restored_size(record)
    }
}
