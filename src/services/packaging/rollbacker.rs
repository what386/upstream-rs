use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::models::common::enums::CompressionLevel;
use crate::models::upstream::Package;
use crate::models::upstream::config::RollbackConfig;
use crate::services::integration::ShellManager;
use crate::services::packaging::PackageRemover;
use crate::services::packaging::disk_impact::{
    ByteEstimate, DiskImpact, SignedByteEstimate, SizeConfidence,
};
use crate::storage::{
    database::PackageDatabase,
    rollback::{RollbackRecord, RollbackSource, RollbackStorage},
    system::config::ConfigStorage,
};
use crate::utils::filesystem::safe_move;
use crate::utils::static_paths::UpstreamPaths;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

/// Coordinates rollback workflows while owning the metadata/artifact storage.
pub struct RollbackManager<'a> {
    paths: &'a UpstreamPaths,
    rollback_storage: RollbackStorage,
}

#[derive(Debug, Clone, Copy)]
struct RollbackCaptureOptions {
    compression_level: CompressionLevel,
    stored_artifacts: usize,
}

impl<'a> RollbackManager<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Result<Self> {
        let rollback_file = paths.dirs.metadata_dir.join("rollback.json");
        let rollback_storage = RollbackStorage::new(&rollback_file, &paths.state.rollback_dir)?;

        Ok(Self {
            paths,
            rollback_storage,
        })
    }

    pub fn capture_from_installed<H>(
        &mut self,
        package: &Package,
        source: RollbackSource,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let Some(options) = Self::capture_options(self.paths)? else {
            return Ok(());
        };

        let install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        if !install_path.exists() {
            return Err(anyhow!(
                "Package '{}' install path does not exist: {}",
                package.name,
                install_path.display()
            ));
        }

        self.capture_path(
            package,
            install_path,
            package.icon_path.as_deref(),
            source,
            options,
            message_callback,
        )
    }

    pub fn capture_backup_path<H>(
        &mut self,
        package: &Package,
        backup_path: &Path,
        source: RollbackSource,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        self.capture_backup(
            package,
            backup_path,
            package.icon_path.as_deref(),
            source,
            message_callback,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn capture_backup<H>(
        &mut self,
        package: &Package,
        backup_path: &Path,
        icon_source: Option<&Path>,
        source: RollbackSource,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let Some(options) = Self::capture_options(self.paths)? else {
            return Ok(());
        };

        self.capture_path(
            package,
            backup_path,
            icon_source,
            source,
            options,
            message_callback,
        )
    }

    fn capture_options(paths: &UpstreamPaths) -> Result<Option<RollbackCaptureOptions>> {
        let config = ConfigStorage::new(&paths.config.config_file)?;
        let rollback = &config.get_config().rollback;

        if !rollback.enabled {
            return Ok(None);
        }

        Ok(Some(RollbackCaptureOptions {
            compression_level: rollback.compression_level,
            stored_artifacts: effective_stored_artifacts(rollback),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_path<H>(
        &mut self,
        package: &Package,
        artifact_path: &Path,
        icon_source: Option<&Path>,
        source: RollbackSource,
        options: RollbackCaptureOptions,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        message!(
            message_callback,
            "Capturing rollback artifact for '{}' ...",
            package.name
        );

        let capture = self.rollback_storage.capture_artifact(
            package,
            artifact_path,
            icon_source,
            source,
            options.compression_level,
            options.stored_artifacts,
        )?;

        message!(
            message_callback,
            "Captured rollback artifact for '{}' at '{}'",
            package.name,
            capture.artifact_path.display()
        );

        for warning in capture.cleanup_warnings {
            message!(message_callback, "Warning: {warning}");
        }

        Ok(())
    }

    pub fn restore_package<H>(
        &mut self,
        package_database: &mut PackageDatabase,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let current = package_database.get_package(package_name)?;
        self.restore_record(
            package_database,
            package_name,
            current.as_ref(),
            message_callback,
        )
    }

    pub fn restore_replaced_package<H>(
        &mut self,
        package_database: &mut PackageDatabase,
        package_name: &str,
        replacement: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        self.restore_record(
            package_database,
            package_name,
            Some(replacement),
            message_callback,
        )
    }

    fn restore_record<H>(
        &mut self,
        package_database: &mut PackageDatabase,
        package_name: &str,
        current: Option<&Package>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let Some(record) = self.rollback_storage.get_record(package_name).cloned() else {
            return Err(anyhow!("No rollback data found for '{}'", package_name));
        };

        let package_settings = package_database.get_package_settings(package_name)?;

        if let Some(current) = current {
            message!(
                message_callback,
                "Removing current installation for '{}' before rollback ...",
                package_name
            );

            PackageRemover::new(self.paths).remove_package_files(current, message_callback)?;
            package_database.remove_package(package_name)?;
        }

        let target_install_path = record
            .package_snapshot
            .install_path
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "Rollback snapshot for '{}' has no install path",
                    package_name
                )
            })?
            .clone();

        if let Some(parent) = target_install_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create install parent '{}'", parent.display())
            })?;
        }

        message!(
            message_callback,
            "Restoring rollback artifact for '{}' ...",
            package_name
        );

        let prepared = self
            .rollback_storage
            .prepare_record(package_name, &record)?;

        let source_path = prepared.artifact_source_path();

        if !source_path.exists() {
            return Err(anyhow!(
                "Rollback artifact is missing for '{}': {}",
                package_name,
                source_path.display()
            ));
        }

        safe_move::move_file_or_dir(source_path, &target_install_path)?;

        if let (Some(icon_source), Some(icon_target)) = (
            prepared.icon_source_path(),
            record.package_snapshot.icon_path.as_ref(),
        ) && icon_source.exists()
        {
            if let Some(parent) = icon_target.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create icon parent '{}'", parent.display())
                })?;
            }

            fs::copy(icon_source, icon_target).with_context(|| {
                format!(
                    "Failed to restore icon from '{}' to '{}'",
                    icon_source.display(),
                    icon_target.display()
                )
            })?;
        }

        drop(prepared);

        if let Some(mut settings) = package_settings {
            settings.package_name = record.package_snapshot.name.clone();
            package_database.upsert_package_with_settings(&record.package_snapshot, &settings)?;
        } else {
            package_database.upsert_package(&record.package_snapshot)?;
        }

        let remover = PackageRemover::new(self.paths);
        remover.restore_runtime_integrations(&record.package_snapshot, message_callback)?;

        ShellManager::new(&self.paths.generated.paths_file)
            .regenerate_paths(package_database, self.paths)?;

        self.rollback_storage
            .consume_record(package_name, &record)?;

        Ok(())
    }

    pub fn prune_package(&mut self, package_name: &str) -> Result<bool> {
        self.rollback_storage.prune_package(package_name)
    }

    pub fn rename_package(&mut self, old_name: &str, new_name: &str) -> Result<bool> {
        self.rollback_storage.rename_package(old_name, new_name)
    }

    pub fn rollback_packages(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .rollback_storage
            .list_records()
            .keys()
            .cloned()
            .collect();

        names.sort();
        names
    }

    pub fn rollback_record(&self, package_name: &str) -> Option<&RollbackRecord> {
        self.rollback_storage.get_record(package_name)
    }

    /// Estimate the rollback-storage delta caused by capturing the currently
    /// installed package and enforcing retention.
    ///
    /// When rollback capture is disabled, this is exactly zero because the
    /// operation will not create or prune any rollback artifacts.
    pub fn estimate_capture_impact(&self, package: &Package) -> Result<SignedByteEstimate> {
        let Some(options) = Self::capture_options(self.paths)? else {
            return Ok(SignedByteEstimate::exact(0));
        };

        let new_capture_size = PackageRemover::new(self.paths)
            .estimate_active_size(package)
            .map(ByteEstimate::estimated)
            .unwrap_or_else(|_| ByteEstimate::unknown());

        let pruned_size = self.estimate_pruned_size(&package.name, options.stored_artifacts)?;

        let Some(new_size) = new_capture_size.bytes else {
            return Ok(SignedByteEstimate::unknown());
        };

        let delta = i128::from(new_size)
            .checked_sub(i128::from(pruned_size))
            .ok_or_else(|| anyhow!("Rollback capture impact overflow"))?;

        Ok(SignedByteEstimate {
            bytes: Some(delta),
            confidence: SizeConfidence::Estimated,
        })
    }

    fn estimate_pruned_size(&self, package_name: &str, stored_artifacts: usize) -> Result<u64> {
        self.rollback_storage
            .records_pruned_by_next_capture(package_name, stored_artifacts)
            .iter()
            .try_fold(0_u64, |total, record| {
                total
                    .checked_add(self.rollback_storage.rollback_record_size(record)?)
                    .ok_or_else(|| anyhow!("Rollback size overflow"))
            })
    }

    /// Restore means:
    ///   + restored package
    ///   - current package
    ///   - consumed rollback artifact
    pub fn estimate_restore_impact(
        &self,
        package_database: &PackageDatabase,
        package_name: &str,
    ) -> Option<DiskImpact> {
        let record = self.rollback_storage.get_record(package_name)?;

        let current_size = package_database
            .get_package(package_name)
            .ok()
            .flatten()
            .and_then(|package| {
                PackageRemover::new(self.paths)
                    .estimate_active_size(&package)
                    .ok()
            })
            .unwrap_or(0);

        let rollback_storage_size = self.rollback_storage.rollback_record_size(record).ok()?;
        let restored = self.rollback_storage.restored_size(record).ok()?;
        let restored_size = if restored.exact {
            ByteEstimate::exact(restored.bytes)
        } else {
            ByteEstimate::estimated(restored.bytes)
        };

        let net = restored_size
            .bytes
            .and_then(|restored| {
                i128::from(restored)
                    .checked_sub(i128::from(current_size))
                    .and_then(|value| value.checked_sub(i128::from(rollback_storage_size)))
            })
            .map(|bytes| SignedByteEstimate {
                bytes: Some(bytes),
                confidence: restored_size.confidence,
            })
            .unwrap_or_else(SignedByteEstimate::unknown);

        Some(DiskImpact {
            download: ByteEstimate::exact(0),
            net,
        })
    }

    pub fn estimate_prune_impact(&self, package_name: &str) -> Option<DiskImpact> {
        self.rollback_storage.get_record(package_name)?;
        let rollback_dir_size = self
            .rollback_storage
            .package_artifacts_size(package_name)
            .ok()?;

        Some(DiskImpact {
            download: ByteEstimate::exact(0),
            net: SignedByteEstimate::exact(-i128::from(rollback_dir_size)),
        })
    }
}

fn effective_stored_artifacts(config: &RollbackConfig) -> usize {
    config.stored_artifacts.max(1) as usize
}

#[cfg(test)]
mod tests {
    use super::RollbackManager;

    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::services::packaging::disk_impact::SizeConfidence;
    use crate::storage::rollback::{RollbackArtifactFormat, RollbackSource};
    use crate::utils::test_support;

    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-rollback-manager-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn write_rollback_config(
        root: &Path,
        enabled: bool,
        compression_level: &str,
        stored_artifacts: u32,
    ) {
        let paths = test_support::upstream_paths(root);

        fs::create_dir_all(paths.config.config_file.parent().expect("config parent"))
            .expect("create config parent");

        fs::write(
            &paths.config.config_file,
            format!(
                "[rollback]\n\
                 enabled = {enabled}\n\
                 compression_level = \"{compression_level}\"\n\
                 stored_artifacts = {stored_artifacts}\n"
            ),
        )
        .expect("write rollback config");
    }

    fn test_package(root: &Path, name: &str) -> Package {
        let paths = test_support::upstream_paths(root);
        let mut package = Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        package.install_path = Some(paths.install.binaries_dir.join(name));
        package
    }

    #[test]
    fn capture_from_installed_is_noop_when_disabled() {
        let root = temp_root("capture-disabled");
        write_rollback_config(&root, false, "low", 2);
        let paths = test_support::upstream_paths(&root);
        let package = test_package(&root, "tool");

        let mut manager = RollbackManager::new(&paths).expect("rollback manager");
        manager
            .capture_from_installed(&package, RollbackSource::Upgrade, &mut None::<fn(&str)>)
            .expect("disabled capture should be a no-op");

        assert!(manager.rollback_storage.get_records("tool").is_empty());
        assert!(!paths.state.rollback_dir.join("tool").exists());
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn capture_backup_path_is_noop_when_disabled() {
        let root = temp_root("backup-capture-disabled");
        write_rollback_config(&root, false, "low", 2);
        let paths = test_support::upstream_paths(&root);
        let package = test_package(&root, "tool");
        let missing_backup = root.join("missing-backup");

        let mut manager = RollbackManager::new(&paths).expect("rollback manager");
        manager
            .capture_backup_path(
                &package,
                &missing_backup,
                RollbackSource::Remove,
                &mut None::<fn(&str)>,
            )
            .expect("disabled backup capture should be a no-op");

        assert!(manager.rollback_storage.get_records("tool").is_empty());
        assert!(!paths.state.rollback_dir.join("tool").exists());
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn capture_impact_is_zero_when_disabled() {
        let root = temp_root("capture-impact-disabled");
        write_rollback_config(&root, false, "low", 2);
        let paths = test_support::upstream_paths(&root);
        let package = test_package(&root, "tool");
        let manager = RollbackManager::new(&paths).expect("rollback manager");

        let impact = manager
            .estimate_capture_impact(&package)
            .expect("capture impact");

        assert_eq!(impact.bytes, Some(0));
        assert_eq!(impact.confidence, SizeConfidence::Exact);
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn capture_from_installed_retains_multiple_compressed_artifacts() {
        let root = temp_root("compressed-retention");
        write_rollback_config(&root, true, "low", 2);
        let paths = test_support::upstream_paths(&root);
        let package = test_package(&root, "tool");
        let install_path = package.install_path.as_ref().expect("install path");

        fs::create_dir_all(install_path.parent().expect("install parent"))
            .expect("create install parent");

        let mut manager = RollbackManager::new(&paths).expect("rollback manager");

        for contents in ["one", "two", "three"] {
            fs::write(install_path, contents).expect("write install artifact");
            manager
                .capture_from_installed(&package, RollbackSource::Upgrade, &mut None::<fn(&str)>)
                .expect("capture rollback");
        }

        let records = manager.rollback_storage.get_records("tool");
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.artifact_format == RollbackArtifactFormat::TarZstd)
        );

        let latest = records.last().expect("latest rollback record").clone();
        assert!(
            latest
                .artifact_relative_path
                .to_string_lossy()
                .ends_with(".tar.zst")
        );

        let prepared = manager
            .rollback_storage
            .prepare_record("tool", &latest)
            .expect("prepare zstd rollback");

        assert_eq!(
            fs::read(prepared.artifact_source_path()).expect("read prepared artifact"),
            b"three"
        );

        drop(prepared);
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn capture_impact_accounts_for_retention_pruning() {
        let root = temp_root("capture-impact");
        write_rollback_config(&root, true, "low", 1);
        let paths = test_support::upstream_paths(&root);
        let package = test_package(&root, "tool");
        let install_path = package.install_path.as_ref().expect("install path");

        fs::create_dir_all(install_path.parent().expect("install parent"))
            .expect("create install parent");
        fs::write(install_path, vec![1_u8; 64 * 1024]).expect("write install");

        let mut manager = RollbackManager::new(&paths).expect("rollback manager");
        manager
            .capture_from_installed(&package, RollbackSource::Upgrade, &mut None::<fn(&str)>)
            .expect("first capture");

        let impact = manager
            .estimate_capture_impact(&package)
            .expect("capture impact");

        assert!(impact.bytes.is_some());
        assert_eq!(impact.confidence, SizeConfidence::Estimated);
        cleanup(&root).expect("cleanup");
    }
}
