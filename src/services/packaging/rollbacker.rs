use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use tar::{Archive, Builder};

use crate::models::common::enums::CompressionLevel;
use crate::models::upstream::Package;
use crate::models::upstream::config::RollbackConfig;
use crate::services::integration::ShellManager;
use crate::services::packaging::PackageRemover;
use crate::services::packaging::disk_impact::{
    ByteEstimate, DiskImpact, SignedByteEstimate, SizeConfidence, estimate_path_size,
};
use crate::storage::{
    database::PackageDatabase,
    rollback::{RollbackArtifactFormat, RollbackRecord, RollbackSource, RollbackStorage},
    system::config::ConfigStorage,
};
use crate::utils::filesystem::{path_exists_no_follow, safe_move};
use crate::utils::static_paths::UpstreamPaths;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct RollbackManager<'a> {
    paths: &'a UpstreamPaths,
    package_database: &'a mut PackageDatabase,
    rollback_storage: &'a mut RollbackStorage,
}

#[derive(Debug, Clone, Copy)]
struct RollbackCaptureOptions {
    compression_level: CompressionLevel,
    stored_artifacts: usize,
}

impl<'a> RollbackManager<'a> {
    pub fn rollback_file_path(paths: &UpstreamPaths) -> PathBuf {
        paths.dirs.metadata_dir.join("rollback.json")
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

    pub fn new(
        paths: &'a UpstreamPaths,
        package_database: &'a mut PackageDatabase,
        rollback_storage: &'a mut RollbackStorage,
    ) -> Self {
        Self {
            paths,
            package_database,
            rollback_storage,
        }
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

        let record = Self::capture_artifact_from_path(
            self.paths,
            package,
            install_path,
            package.icon_path.as_deref(),
            source,
            options,
            message_callback,
        )?;

        let pruned = match self.rollback_storage.push_record(
            &package.name,
            record.clone(),
            options.stored_artifacts,
        ) {
            Ok(pruned) => pruned,
            Err(storage_error) => {
                return cleanup_failed_capture(
                    self.paths,
                    &package.name,
                    &record,
                    storage_error,
                );
            }
        };

        for record in pruned {
            if let Err(error) = delete_record_artifacts(self.paths, &package.name, &record) {
                message!(
                    message_callback,
                    "Warning: failed to delete pruned rollback artifact for '{}': {}",
                    package.name,
                    error
                );
            }
        }

        Ok(())
    }

    pub fn capture_backup_path<H>(
        paths: &UpstreamPaths,
        rollback_storage: &mut RollbackStorage,
        package: &Package,
        backup_path: &Path,
        source: RollbackSource,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        Self::capture_backup_path_with_icon_source(
            paths,
            rollback_storage,
            package,
            backup_path,
            package.icon_path.as_deref(),
            source,
            message_callback,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn capture_backup_path_with_icon_source<H>(
        paths: &UpstreamPaths,
        rollback_storage: &mut RollbackStorage,
        package: &Package,
        backup_path: &Path,
        icon_source: Option<&Path>,
        source: RollbackSource,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let Some(options) = Self::capture_options(paths)? else {
            return Ok(());
        };

        let record = Self::capture_artifact_from_path(
            paths,
            package,
            backup_path,
            icon_source,
            source,
            options,
            message_callback,
        )?;

        let pruned = match rollback_storage.push_record(
            &package.name,
            record.clone(),
            options.stored_artifacts,
        ) {
            Ok(pruned) => pruned,
            Err(storage_error) => {
                return cleanup_failed_capture(
                    paths,
                    &package.name,
                    &record,
                    storage_error,
                );
            }
        };

        for record in pruned {
            if let Err(error) = delete_record_artifacts(paths, &package.name, &record) {
                message!(
                    message_callback,
                    "Warning: failed to delete pruned rollback artifact for '{}': {}",
                    package.name,
                    error
                );
            }
        }

        Ok(())
    }

    fn capture_artifact_from_path<H>(
        paths: &UpstreamPaths,
        package: &Package,
        artifact_path: &Path,
        icon_source: Option<&Path>,
        source: RollbackSource,
        options: RollbackCaptureOptions,
        message_callback: &mut Option<H>,
    ) -> Result<RollbackRecord>
    where
        H: FnMut(&str),
    {
        let artifact_name = artifact_path.file_name().ok_or_else(|| {
            anyhow!(
                "Rollback artifact path '{}' has no final file name",
                artifact_path.display()
            )
        })?;

        let package_rollback_dir = paths.state.rollback_dir.join(&package.name);
        fs::create_dir_all(&package_rollback_dir).context(format!(
            "Failed to create rollback directory '{}'",
            package_rollback_dir.display()
        ))?;

        let capture_id = rollback_capture_id(&source);
        let capture_dir = package_rollback_dir.join(&capture_id);

        fs::create_dir_all(&capture_dir).context(format!(
            "Failed to create rollback capture directory '{}'",
            capture_dir.display()
        ))?;

        let artifact_entry_path = PathBuf::from("artifact").join(artifact_name);
        let rollback_artifact = capture_dir.join(&artifact_entry_path);

        if let Some(parent) = rollback_artifact.parent() {
            fs::create_dir_all(parent).context(format!(
                "Failed to create rollback artifact parent '{}'",
                parent.display()
            ))?;
        }

        message!(
            message_callback,
            "Capturing rollback artifact for '{}' at '{}'",
            package.name,
            rollback_artifact.display()
        );

        let archive_path = package_rollback_dir.join(format!("{capture_id}.tgz"));

        let capture_result = (|| {
            safe_move::copy_file_or_dir(artifact_path, &rollback_artifact)?;

            let icon_entry_path = capture_icon(
                paths,
                icon_source,
                &capture_dir,
            )?;

            let created_at = Utc::now();

            if matches!(
                options.compression_level,
                CompressionLevel::None
            ) {
                return Ok(RollbackRecord {
                    package_snapshot: package.clone(),
                    artifact_relative_path: path_relative_to(
                        &paths.state.rollback_dir,
                        &rollback_artifact,
                    )?,
                    icon_relative_path: icon_entry_path
                        .as_ref()
                        .map(|entry| {
                            path_relative_to(
                                &paths.state.rollback_dir,
                                &capture_dir.join(entry),
                            )
                        })
                        .transpose()?,
                    artifact_format: RollbackArtifactFormat::Raw,
                    artifact_entry_path: None,
                    icon_entry_path: None,
                    source,
                    created_at,
                });
            }

            compress_capture_dir(
                &capture_dir,
                &archive_path,
                options.compression_level,
            )?;

            fs::remove_dir_all(&capture_dir).context(format!(
                "Failed to remove rollback staging directory '{}'",
                capture_dir.display()
            ))?;

            Ok(RollbackRecord {
                package_snapshot: package.clone(),
                artifact_relative_path: path_relative_to(
                    &paths.state.rollback_dir,
                    &archive_path,
                )?,
                icon_relative_path: None,
                artifact_format: RollbackArtifactFormat::Tgz,
                artifact_entry_path: Some(artifact_entry_path),
                icon_entry_path,
                source,
                created_at,
            })
        })();

        if let Err(capture_error) = capture_result {
            let cleanup_result = remove_file_or_dir_if_exists(&capture_dir)
                .and_then(|()| remove_file_or_dir_if_exists(&archive_path))
                .and_then(|()| {
                    cleanup_empty_package_rollback_dir(
                        paths,
                        &package.name,
                    )
                });

            return match cleanup_result {
                Ok(()) => Err(capture_error),

                Err(cleanup_error) => Err(anyhow!(
                    "{capture_error:#}. Failed to clean incomplete rollback capture: {cleanup_error:#}"
                )),
            };
        }

        capture_result
    }

    pub fn restore_package<H>(
        &mut self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let current = self.package_database.get_package(package_name)?;

        self.restore_record(
            package_name,
            current.as_ref(),
            message_callback,
        )
    }

    pub fn restore_replaced_package<H>(
        &mut self,
        package_name: &str,
        replacement: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        self.restore_record(
            package_name,
            Some(replacement),
            message_callback,
        )
    }

    fn restore_record<H>(
        &mut self,
        package_name: &str,
        current: Option<&Package>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let Some(record) = self
            .rollback_storage
            .get_record(package_name)
            .cloned()
        else {
            return Err(anyhow!(
                "No rollback data found for '{}'",
                package_name
            ));
        };

        let package_settings = self
            .package_database
            .get_package_settings(package_name)?;

        if let Some(current) = current {
            message!(
                message_callback,
                "Removing current installation for '{}' before rollback ...",
                package_name
            );

            let remover = PackageRemover::new(self.paths);

            remover.remove_package_files(
                current,
                message_callback,
            )?;

            self.package_database
                .remove_package(package_name)?;
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
            fs::create_dir_all(parent).context(format!(
                "Failed to create install parent '{}'",
                parent.display()
            ))?;
        }

        message!(
            message_callback,
            "Restoring rollback artifact for '{}' ...",
            package_name
        );

        let extracted_dir = match record.artifact_format {
            RollbackArtifactFormat::Raw => None,

            RollbackArtifactFormat::Tgz => {
                Some(extract_record_archive(
                    self.paths,
                    package_name,
                    &record,
                )?)
            }
        };

        let source_path = record_artifact_source_path(
            self.paths,
            &record,
            extracted_dir.as_deref(),
        )?;

        if !source_path.exists() {
            return Err(anyhow!(
                "Rollback artifact is missing for '{}': {}",
                package_name,
                source_path.display()
            ));
        }

        safe_move::move_file_or_dir(
            &source_path,
            &target_install_path,
        )?;

        let icon_source = record_icon_source_path(
            self.paths,
            &record,
            extracted_dir.as_deref(),
        )?;

        if let (Some(icon_source), Some(icon_target)) = (
            icon_source.as_ref(),
            record.package_snapshot.icon_path.as_ref(),
        ) && icon_source.exists()
        {
            if let Some(parent) = icon_target.parent() {
                fs::create_dir_all(parent).context(format!(
                    "Failed to create icon parent '{}'",
                    parent.display()
                ))?;
            }

            fs::copy(icon_source, icon_target).context(format!(
                "Failed to restore icon from '{}' to '{}'",
                icon_source.display(),
                icon_target.display()
            ))?;
        }

        if let Some(mut settings) = package_settings {
            settings.package_name =
                record.package_snapshot.name.clone();

            self.package_database
                .upsert_package_with_settings(
                    &record.package_snapshot,
                    &settings,
                )?;
        } else {
            self.package_database
                .upsert_package(
                    &record.package_snapshot,
                )?;
        }

        let remover = PackageRemover::new(self.paths);

        remover.restore_runtime_integrations(
            &record.package_snapshot,
            message_callback,
        )?;

        ShellManager::new(
            &self.paths.generated.paths_file,
        )
        .regenerate_paths(
            self.package_database,
            self.paths,
        )?;

        self.rollback_storage
            .remove_record(package_name)?;

        delete_record_artifacts(
            self.paths,
            package_name,
            &record,
        )?;

        if let Some(extracted_dir) = extracted_dir
            && extracted_dir.exists()
        {
            let _ = fs::remove_dir_all(
                extracted_dir,
            );
        }

        Ok(())
    }

    pub fn prune_package(
        &mut self,
        package_name: &str,
    ) -> Result<bool> {
        let removed = self
            .rollback_storage
            .remove_all_records(package_name)?;

        for record in &removed {
            delete_record_artifacts(
                self.paths,
                package_name,
                record,
            )?;
        }

        cleanup_empty_package_rollback_dir(
            self.paths,
            package_name,
        )?;

        Ok(!removed.is_empty())
    }

    pub fn rename_package(
        paths: &UpstreamPaths,
        rollback_storage: &mut RollbackStorage,
        old_name: &str,
        new_name: &str,
    ) -> Result<bool> {
        let source_dir =
            paths.state.rollback_dir.join(old_name);

        let destination_dir =
            paths.state.rollback_dir.join(new_name);

        if path_exists_no_follow(&destination_dir)? {
            return Err(anyhow!(
                "Rollback directory already exists for package '{}'",
                new_name
            ));
        }

        let moved_directory =
            if path_exists_no_follow(&source_dir)? {
                fs::rename(
                    &source_dir,
                    &destination_dir,
                )
                .with_context(|| {
                    format!(
                        "Failed to rename rollback directory '{}' to '{}'",
                        source_dir.display(),
                        destination_dir.display()
                    )
                })?;

                true
            } else {
                false
            };

        match rollback_storage
            .rename_package(old_name, new_name)
        {
            Ok(renamed_metadata) => {
                Ok(
                    moved_directory
                        || renamed_metadata,
                )
            }

            Err(error) => {
                if moved_directory
                    && let Err(rollback_error) =
                        fs::rename(
                            &destination_dir,
                            &source_dir,
                        )
                {
                    return Err(anyhow!(
                        "Failed to rename rollback metadata from '{}' to '{}': {}. Directory rollback also failed: {}",
                        old_name,
                        new_name,
                        error,
                        rollback_error
                    ));
                }

                Err(error)
            }
        }
    }

    pub fn rollback_packages(
        &self,
    ) -> Vec<String> {
        let mut names: Vec<String> = self
            .rollback_storage
            .list_records()
            .keys()
            .cloned()
            .collect();

        names.sort();

        names
    }

    pub fn rollback_record(
        &self,
        package_name: &str,
    ) -> Option<&RollbackRecord> {
        self.rollback_storage
            .get_record(package_name)
    }

    /// Estimate the rollback-storage delta caused by capturing the currently
    /// installed package and enforcing retention.
    ///
    /// When rollback capture is disabled, this is exactly zero because the
    /// operation will not create or prune any rollback artifacts.
    ///
    /// When enabled, the new capture is intentionally marked Estimated:
    /// - raw captures are close to active size;
    /// - TGZ captures are normally smaller than active size.
    ///
    /// This is conservative and accounts for both sides:
    /// the new snapshot added and old snapshots pruned.
    pub fn estimate_capture_impact(
        &self,
        package: &Package,
    ) -> Result<SignedByteEstimate> {
        let Some(options) =
            Self::capture_options(self.paths)?
        else {
            return Ok(
                SignedByteEstimate::exact(0),
            );
        };

        let new_capture_size =
            PackageRemover::new(self.paths)
                .estimate_active_size(package)
                .map(ByteEstimate::estimated)
                .unwrap_or_else(|_| {
                    ByteEstimate::unknown()
                });

        let pruned_size =
            self.estimate_pruned_size(
                &package.name,
                options.stored_artifacts,
            )?;

        let Some(new_size) =
            new_capture_size.bytes
        else {
            return Ok(
                SignedByteEstimate::unknown(),
            );
        };

        let delta = i128::from(new_size)
            .checked_sub(
                i128::from(pruned_size),
            )
            .ok_or_else(|| {
                anyhow!(
                    "Rollback capture impact overflow"
                )
            })?;

        Ok(SignedByteEstimate {
            bytes: Some(delta),
            confidence:
                SizeConfidence::Estimated,
        })
    }

    fn estimate_pruned_size(
        &self,
        package_name: &str,
        stored_artifacts: usize,
    ) -> Result<u64> {
        self.rollback_storage
            .records_pruned_by_next_push(
                package_name,
                stored_artifacts,
            )
            .iter()
            .try_fold(
                0_u64,
                |total, record| {
                    total
                        .checked_add(
                            rollback_record_size(
                                self.paths,
                                record,
                            )?,
                        )
                        .ok_or_else(|| {
                            anyhow!(
                                "Rollback size overflow"
                            )
                        })
                },
            )
    }

    /// Restore means:
    ///   + restored package
    ///   - current package
    ///   - consumed rollback artifact
    pub fn estimate_restore_impact(
        &self,
        package_name: &str,
    ) -> Option<DiskImpact> {
        let record = self
            .rollback_storage
            .get_record(package_name)?;

        let current_size = self
            .package_database
            .get_package(package_name)
            .ok()
            .flatten()
            .and_then(|package| {
                PackageRemover::new(self.paths)
                    .estimate_active_size(
                        &package,
                    )
                    .ok()
            })
            .unwrap_or(0);

        let rollback_storage_size =
            rollback_record_size(
                self.paths,
                record,
            )
            .ok()?;

        let restored_size =
            estimate_record_restored_size(
                self.paths,
                record,
            )
            .ok()?;

        let net = restored_size
            .bytes
            .and_then(|restored| {
                i128::from(restored)
                    .checked_sub(
                        i128::from(current_size),
                    )
                    .and_then(|value| {
                        value.checked_sub(
                            i128::from(
                                rollback_storage_size,
                            ),
                        )
                    })
            })
            .map(|bytes| {
                SignedByteEstimate {
                    bytes: Some(bytes),
                    confidence:
                        restored_size.confidence,
                }
            })
            .unwrap_or_else(
                SignedByteEstimate::unknown,
            );

        Some(DiskImpact {
            download: ByteEstimate::exact(0),
            net,
        })
    }

    pub fn estimate_prune_impact(
        &self,
        package_name: &str,
    ) -> Option<DiskImpact> {
        self.rollback_storage
            .get_record(package_name)?;

        let rollback_dir_size =
            estimate_path_size(
                &self
                    .paths
                    .state
                    .rollback_dir
                    .join(package_name),
            )
            .ok()?;

        Some(DiskImpact {
            download: ByteEstimate::exact(0),
            net: SignedByteEstimate::exact(
                -i128::from(
                    rollback_dir_size,
                ),
            ),
        })
    }
}

fn rollback_record_size(
    paths: &UpstreamPaths,
    record: &RollbackRecord,
) -> Result<u64> {
    let artifact_path = paths
        .state
        .rollback_dir
        .join(
            &record.artifact_relative_path,
        );

    let mut total =
        estimate_path_size(&artifact_path)?;

    if matches!(
        record.artifact_format,
        RollbackArtifactFormat::Raw
    ) && let Some(icon_path) =
        record.icon_relative_path.as_ref()
    {
        total = total
            .checked_add(
                estimate_path_size(
                    &paths
                        .state
                        .rollback_dir
                        .join(icon_path),
                )?,
            )
            .ok_or_else(|| {
                anyhow!(
                    "Rollback size overflow"
                )
            })?;
    }

    Ok(total)
}

/// Estimate the package payload restored by a record.
///
/// Raw rollback data is already on disk and can be measured directly.
/// TGZ data is measured from tar entry sizes and marked Estimated because
/// extracted filesystem allocation can differ from logical archive bytes.
fn estimate_record_restored_size(
    paths: &UpstreamPaths,
    record: &RollbackRecord,
) -> Result<ByteEstimate> {
    match record.artifact_format {
        RollbackArtifactFormat::Raw => {
            let path = paths
                .state
                .rollback_dir
                .join(
                    &record
                        .artifact_relative_path,
                );

            Ok(ByteEstimate::exact(
                estimate_path_size(&path)?,
            ))
        }

        RollbackArtifactFormat::Tgz => {
            let archive_path = paths
                .state
                .rollback_dir
                .join(
                    &record
                        .artifact_relative_path,
                );

            let archive_file =
                File::open(&archive_path)
                    .with_context(|| {
                        format!(
                            "Failed to open rollback archive '{}'",
                            archive_path.display()
                        )
                    })?;

            let decoder =
                GzDecoder::new(archive_file);

            let mut archive =
                Archive::new(decoder);

            let artifact_root = record
                .artifact_entry_path
                .as_deref()
                .and_then(|path| {
                    path.components().next()
                })
                .map(|component| {
                    PathBuf::from(
                        component.as_os_str(),
                    )
                })
                .unwrap_or_else(|| {
                    PathBuf::from("artifact")
                });

            let mut total = 0_u64;

            for entry in archive
                .entries()
                .context(
                    "Failed to read rollback archive entries",
                )?
            {
                let entry = entry.context(
                    "Failed to read rollback archive entry",
                )?;

                let path = entry
                    .path()
                    .context(
                        "Failed to read rollback archive entry path",
                    )?;

                if !path.starts_with(
                    &artifact_root,
                ) {
                    continue;
                }

                if entry
                    .header()
                    .entry_type()
                    .is_file()
                {
                    total = total
                        .checked_add(
                            entry
                                .header()
                                .size()?,
                        )
                        .ok_or_else(|| {
                            anyhow!(
                                "Restored size overflow"
                            )
                        })?;
                }
            }

            Ok(ByteEstimate::estimated(
                total,
            ))
        }
    }
}

fn cleanup_failed_capture(
    paths: &UpstreamPaths,
    package_name: &str,
    record: &RollbackRecord,
    storage_error: anyhow::Error,
) -> Result<()> {
    match delete_record_artifacts(
        paths,
        package_name,
        record,
    ) {
        Ok(()) => Err(storage_error)
            .context(
                "Failed to persist rollback record",
            ),

        Err(cleanup_error) => Err(anyhow!(
            "Failed to persist rollback record: {storage_error:#}. Failed to clean unrecorded rollback artifact: {cleanup_error:#}"
        )),
    }
}

fn path_relative_to(
    base: &Path,
    full: &Path,
) -> Result<PathBuf> {
    full.strip_prefix(base)
        .map(Path::to_path_buf)
        .map_err(|_| {
            anyhow!(
                "Path '{}' is not under '{}'",
                full.display(),
                base.display()
            )
        })
}

fn effective_stored_artifacts(
    config: &RollbackConfig,
) -> usize {
    config.stored_artifacts.max(1) as usize
}

fn rollback_capture_id(
    source: &RollbackSource,
) -> String {
    let source_label = match source {
        RollbackSource::Upgrade => "upgrade",
        RollbackSource::Reinstall => "reinstall",
        RollbackSource::Remove => "remove",
    };

    let timestamp = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| {
            Utc::now().timestamp_micros()
                * 1_000
        });

    format!(
        "{timestamp}-{source_label}"
    )
}

fn capture_icon(
    paths: &UpstreamPaths,
    icon_path: Option<&Path>,
    capture_dir: &Path,
) -> Result<Option<PathBuf>> {
    let Some(icon_path) = icon_path else {
        return Ok(None);
    };

    if !icon_path.exists() {
        return Ok(None);
    }

    let icon_name = icon_path
        .file_name()
        .ok_or_else(|| {
            anyhow!(
                "Icon path '{}' has no file name",
                icon_path.display()
            )
        })?;

    let icon_entry_path =
        PathBuf::from("icon").join(
            format!(
                "icon-{}",
                icon_name.to_string_lossy()
            ),
        );

    let icon_backup =
        capture_dir.join(
            &icon_entry_path,
        );

    if let Some(parent) =
        icon_backup.parent()
    {
        fs::create_dir_all(parent)
            .context(format!(
                "Failed to create rollback icon parent '{}'",
                parent.display()
            ))?;
    }

    fs::copy(
        icon_path,
        &icon_backup,
    )
    .context(format!(
        "Failed to copy icon '{}' to '{}'",
        icon_path.display(),
        icon_backup.display()
    ))?;

    path_relative_to(
        &paths.state.rollback_dir,
        &icon_backup,
    )?;

    Ok(Some(icon_entry_path))
}

fn gzip_level(
    level: CompressionLevel,
) -> Compression {
    match level {
        CompressionLevel::None => {
            Compression::none()
        }

        CompressionLevel::Low => {
            Compression::fast()
        }

        CompressionLevel::High => {
            Compression::best()
        }
    }
}

fn compress_capture_dir(
    capture_dir: &Path,
    archive_path: &Path,
    level: CompressionLevel,
) -> Result<()> {
    let archive_file =
        File::create(archive_path)
            .with_context(|| {
                format!(
                    "Failed to create rollback archive '{}'",
                    archive_path.display()
                )
            })?;

    let encoder =
        GzEncoder::new(
            archive_file,
            gzip_level(level),
        );

    let mut builder =
        Builder::new(encoder);

    append_capture_entry(
        &mut builder,
        capture_dir,
        Path::new("artifact"),
    )?;

    let icon_dir =
        capture_dir.join("icon");

    if icon_dir.exists() {
        append_capture_entry(
            &mut builder,
            capture_dir,
            Path::new("icon"),
        )?;
    }

    let encoder = builder
        .into_inner()
        .context(
            "Failed to finish rollback tar archive",
        )?;

    encoder
        .finish()
        .context(
            "Failed to finish rollback gzip archive",
        )?;

    Ok(())
}

fn append_capture_entry(
    builder: &mut Builder<GzEncoder<File>>,
    capture_dir: &Path,
    entry: &Path,
) -> Result<()> {
    let full_path =
        capture_dir.join(entry);

    if full_path.is_dir() {
        builder
            .append_dir_all(
                entry,
                &full_path,
            )
            .with_context(|| {
                format!(
                    "Failed to archive '{}'",
                    full_path.display()
                )
            })?;
    } else if full_path.is_file() {
        builder
            .append_path_with_name(
                &full_path,
                entry,
            )
            .with_context(|| {
                format!(
                    "Failed to archive '{}'",
                    full_path.display()
                )
            })?;
    }

    Ok(())
}

fn extract_record_archive(
    paths: &UpstreamPaths,
    package_name: &str,
    record: &RollbackRecord,
) -> Result<PathBuf> {
    let archive_path = paths
        .state
        .rollback_dir
        .join(
            &record.artifact_relative_path,
        );

    if !archive_path.exists() {
        return Err(anyhow!(
            "Rollback archive is missing for '{}': {}",
            package_name,
            archive_path.display()
        ));
    }

    let extract_dir = paths
        .state
        .rollback_dir
        .join(format!(
            ".restore-{}-{}",
            package_name,
            std::process::id()
        ));

    if extract_dir.exists() {
        fs::remove_dir_all(
            &extract_dir,
        )
        .context(format!(
            "Failed to clear rollback extraction directory '{}'",
            extract_dir.display()
        ))?;
    }

    fs::create_dir_all(
        &extract_dir,
    )
    .context(format!(
        "Failed to create rollback extraction directory '{}'",
        extract_dir.display()
    ))?;

    let archive_file =
        File::open(&archive_path)
            .with_context(|| {
                format!(
                    "Failed to open rollback archive '{}'",
                    archive_path.display()
                )
            })?;

    let decoder =
        GzDecoder::new(archive_file);

    let mut archive =
        Archive::new(decoder);

    for entry in archive
        .entries()
        .context(
            "Failed to read rollback archive entries",
        )?
    {
        let mut entry = entry.context(
            "Failed to read rollback archive entry",
        )?;

        let entry_path = entry
            .path()
            .context(
                "Failed to read rollback archive entry path",
            )?
            .into_owned();

        if !is_safe_archive_entry(
            &entry_path,
        ) {
            return Err(anyhow!(
                "Rollback archive contains unsafe path '{}'",
                entry_path.display()
            ));
        }

        entry
            .unpack_in(&extract_dir)
            .with_context(|| {
                format!(
                    "Failed to extract rollback archive entry '{}' into '{}'",
                    entry_path.display(),
                    extract_dir.display()
                )
            })?;
    }

    Ok(extract_dir)
}

fn is_safe_archive_entry(
    path: &Path,
) -> bool {
    path.is_relative()
        && !path.components().any(
            |component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                )
            },
        )
}

fn record_artifact_source_path(
    paths: &UpstreamPaths,
    record: &RollbackRecord,
    extracted_dir: Option<&Path>,
) -> Result<PathBuf> {
    match record.artifact_format {
        RollbackArtifactFormat::Raw => {
            Ok(
                paths
                    .state
                    .rollback_dir
                    .join(
                        &record
                            .artifact_relative_path,
                    ),
            )
        }

        RollbackArtifactFormat::Tgz => {
            let extract_dir =
                extracted_dir.ok_or_else(
                    || {
                        anyhow!(
                            "Rollback archive was not extracted"
                        )
                    },
                )?;

            let entry = record
                .artifact_entry_path
                .as_ref()
                .ok_or_else(|| {
                    anyhow!(
                        "Rollback archive record is missing artifact entry path"
                    )
                })?;

            Ok(
                extract_dir.join(entry),
            )
        }
    }
}

fn record_icon_source_path(
    paths: &UpstreamPaths,
    record: &RollbackRecord,
    extracted_dir: Option<&Path>,
) -> Result<Option<PathBuf>> {
    match record.artifact_format {
        RollbackArtifactFormat::Raw => {
            Ok(
                record
                    .icon_relative_path
                    .as_ref()
                    .map(|path| {
                        paths
                            .state
                            .rollback_dir
                            .join(path)
                    }),
            )
        }

        RollbackArtifactFormat::Tgz => {
            let Some(entry) =
                record.icon_entry_path.as_ref()
            else {
                return Ok(None);
            };

            let extract_dir =
                extracted_dir.ok_or_else(
                    || {
                        anyhow!(
                            "Rollback archive was not extracted"
                        )
                    },
                )?;

            Ok(Some(
                extract_dir.join(entry),
            ))
        }
    }
}

fn delete_record_artifacts(
    paths: &UpstreamPaths,
    package_name: &str,
    record: &RollbackRecord,
) -> Result<()> {
    match record.artifact_format {
        RollbackArtifactFormat::Raw => {
            let artifact_path = paths
                .state
                .rollback_dir
                .join(
                    &record
                        .artifact_relative_path,
                );

            remove_file_or_dir_if_exists(
                &artifact_path,
            )?;

            if let Some(icon_path) =
                record.icon_relative_path.as_ref()
            {
                let icon_path = paths
                    .state
                    .rollback_dir
                    .join(icon_path);

                remove_file_or_dir_if_exists(
                    &icon_path,
                )?;

                cleanup_empty_rollback_ancestors(
                    &paths
                        .state
                        .rollback_dir
                        .join(package_name),
                    icon_path.parent(),
                )?;
            }

            cleanup_empty_rollback_ancestors(
                &paths
                    .state
                    .rollback_dir
                    .join(package_name),
                artifact_path.parent(),
            )?;
        }

        RollbackArtifactFormat::Tgz => {
            remove_file_or_dir_if_exists(
                &paths
                    .state
                    .rollback_dir
                    .join(
                        &record
                            .artifact_relative_path,
                    ),
            )?;
        }
    }

    cleanup_empty_package_rollback_dir(
        paths,
        package_name,
    )
}

fn cleanup_empty_rollback_ancestors(
    package_dir: &Path,
    start: Option<&Path>,
) -> Result<()> {
    let Some(mut current) = start else {
        return Ok(());
    };

    while current.starts_with(
        package_dir,
    ) && current != package_dir
    {
        if current.exists()
            && current
                .read_dir()
                .map(|mut entries| {
                    entries.next().is_none()
                })
                .unwrap_or(false)
        {
            fs::remove_dir(current)
                .with_context(|| {
                    format!(
                        "Failed to remove empty rollback directory '{}'",
                        current.display()
                    )
                })?;
        }

        let Some(parent) =
            current.parent()
        else {
            break;
        };

        current = parent;
    }

    Ok(())
}

fn cleanup_empty_package_rollback_dir(
    paths: &UpstreamPaths,
    package_name: &str,
) -> Result<()> {
    let package_dir = paths
        .state
        .rollback_dir
        .join(package_name);

    if package_dir.exists()
        && package_dir
            .read_dir()
            .map(|mut entries| {
                entries.next().is_none()
            })
            .unwrap_or(false)
    {
        fs::remove_dir(
            &package_dir,
        )
        .context(format!(
            "Failed to remove empty rollback directory '{}'",
            package_dir.display()
        ))?;
    }

    Ok(())
}

fn remove_file_or_dir_if_exists(
    path: &Path,
) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| {
                format!(
                    "Failed to remove directory '{}'",
                    path.display()
                )
            })?;
    } else if path.is_file() {
        fs::remove_file(path)
            .with_context(|| {
                format!(
                    "Failed to remove file '{}'",
                    path.display()
                )
            })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RollbackManager;

    use crate::models::common::enums::{
        Channel, Filetype, Provider,
    };
    use crate::models::upstream::Package;
    use crate::services::packaging::disk_impact::SizeConfidence;
    use crate::storage::rollback::{
        RollbackArtifactFormat,
        RollbackSource,
    };
    use crate::storage::{
        database::PackageDatabase,
        rollback::RollbackStorage,
    };
    use crate::utils::test_support;

    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root(
            "upstream-rollback-manager-test",
            name,
        )
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
        let paths =
            test_support::upstream_paths(root);

        fs::create_dir_all(
            paths
                .config
                .config_file
                .parent()
                .expect("config parent"),
        )
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

    fn test_package(
        root: &Path,
        name: &str,
    ) -> Package {
        let paths =
            test_support::upstream_paths(root);

        let mut package =
            Package::with_defaults(
                name.to_string(),
                format!("owner/{name}"),
                Filetype::Binary,
                None,
                None,
                Channel::Stable,
                Provider::Github,
                None,
            );

        package.install_path = Some(
            paths
                .install
                .binaries_dir
                .join(name),
        );

        package
    }

    #[test]
    fn capture_from_installed_is_noop_when_disabled() {
        let root =
            temp_root("capture-disabled");

        write_rollback_config(
            &root,
            false,
            "low",
            2,
        );

        let paths =
            test_support::upstream_paths(
                &root,
            );

        let mut package_database =
            PackageDatabase::open(
                &paths
                    .metadata
                    .packages_database_file,
            )
            .expect("package storage");

        let rollback_file =
            RollbackManager::rollback_file_path(
                &paths,
            );

        let mut rollback_storage =
            RollbackStorage::new(
                &rollback_file,
            )
            .expect("rollback storage");

        let package =
            test_package(
                &root,
                "tool",
            );

        /*
         * Intentionally do not create the install path.
         *
         * Disabled rollback capture must return before validating or
         * touching the package payload.
         */
        {
            let mut manager =
                RollbackManager::new(
                    &paths,
                    &mut package_database,
                    &mut rollback_storage,
                );

            manager
                .capture_from_installed(
                    &package,
                    RollbackSource::Upgrade,
                    &mut None::<fn(&str)>,
                )
                .expect(
                    "disabled capture should be a no-op",
                );
        }

        assert!(
            rollback_storage
                .get_records("tool")
                .is_empty()
        );

        assert!(
            !paths
                .state
                .rollback_dir
                .join("tool")
                .exists()
        );

        cleanup(&root)
            .expect("cleanup");
    }

    #[test]
    fn capture_backup_path_is_noop_when_disabled() {
        let root =
            temp_root(
                "backup-capture-disabled",
            );

        write_rollback_config(
            &root,
            false,
            "low",
            2,
        );

        let paths =
            test_support::upstream_paths(
                &root,
            );

        let rollback_file =
            RollbackManager::rollback_file_path(
                &paths,
            );

        let mut rollback_storage =
            RollbackStorage::new(
                &rollback_file,
            )
            .expect("rollback storage");

        let package =
            test_package(
                &root,
                "tool",
            );

        let missing_backup =
            root.join(
                "missing-backup",
            );

        /*
         * The backup path also intentionally does not exist. Disabled
         * capture should return before attempting to inspect or copy it.
         */
        RollbackManager::capture_backup_path(
            &paths,
            &mut rollback_storage,
            &package,
            &missing_backup,
            RollbackSource::Remove,
            &mut None::<fn(&str)>,
        )
        .expect(
            "disabled backup capture should be a no-op",
        );

        assert!(
            rollback_storage
                .get_records("tool")
                .is_empty()
        );

        assert!(
            !paths
                .state
                .rollback_dir
                .join("tool")
                .exists()
        );

        cleanup(&root)
            .expect("cleanup");
    }

    #[test]
    fn capture_impact_is_zero_when_disabled() {
        let root =
            temp_root(
                "capture-impact-disabled",
            );

        write_rollback_config(
            &root,
            false,
            "low",
            2,
        );

        let paths =
            test_support::upstream_paths(
                &root,
            );

        let mut package_database =
            PackageDatabase::open(
                &paths
                    .metadata
                    .packages_database_file,
            )
            .expect("package storage");

        let rollback_file =
            RollbackManager::rollback_file_path(
                &paths,
            );

        let mut rollback_storage =
            RollbackStorage::new(
                &rollback_file,
            )
            .expect("rollback storage");

        let package =
            test_package(
                &root,
                "tool",
            );

        let manager =
            RollbackManager::new(
                &paths,
                &mut package_database,
                &mut rollback_storage,
            );

        let impact = manager
            .estimate_capture_impact(
                &package,
            )
            .expect(
                "capture impact",
            );

        assert_eq!(
            impact.bytes,
            Some(0)
        );

        assert_eq!(
            impact.confidence,
            SizeConfidence::Exact
        );

        cleanup(&root)
            .expect("cleanup");
    }

    #[test]
    fn capture_from_installed_retains_multiple_compressed_artifacts() {
        let root =
            temp_root(
                "compressed-retention",
            );

        write_rollback_config(
            &root,
            true,
            "low",
            2,
        );

        let paths =
            test_support::upstream_paths(
                &root,
            );

        let mut package_database =
            PackageDatabase::open(
                &paths
                    .metadata
                    .packages_database_file,
            )
            .expect("package storage");

        let rollback_file =
            RollbackManager::rollback_file_path(
                &paths,
            );

        let mut rollback_storage =
            RollbackStorage::new(
                &rollback_file,
            )
            .expect("rollback storage");

        let package =
            test_package(
                &root,
                "tool",
            );

        let install_path =
            package
                .install_path
                .as_ref()
                .expect(
                    "install path",
                );

        fs::create_dir_all(
            install_path
                .parent()
                .expect(
                    "install parent",
                ),
        )
        .expect(
            "create install parent",
        );

        {
            let mut manager =
                RollbackManager::new(
                    &paths,
                    &mut package_database,
                    &mut rollback_storage,
                );

            for contents in [
                "one",
                "two",
                "three",
            ] {
                fs::write(
                    install_path,
                    contents,
                )
                .expect(
                    "write install artifact",
                );

                manager
                    .capture_from_installed(
                        &package,
                        RollbackSource::Upgrade,
                        &mut None::<fn(&str)>,
                    )
                    .expect(
                        "capture rollback",
                    );
            }
        }

        let records =
            rollback_storage
                .get_records("tool");

        assert_eq!(
            records.len(),
            2
        );

        assert!(
            records
                .iter()
                .all(|record| {
                    record.artifact_format
                        == RollbackArtifactFormat::Tgz
                })
        );

        cleanup(&root)
            .expect("cleanup");
    }

    #[test]
    fn capture_impact_accounts_for_retention_pruning() {
        let root =
            temp_root(
                "capture-impact",
            );

        write_rollback_config(
            &root,
            true,
            "low",
            1,
        );

        let paths =
            test_support::upstream_paths(
                &root,
            );

        let mut package_database =
            PackageDatabase::open(
                &paths
                    .metadata
                    .packages_database_file,
            )
            .expect("package storage");

        let rollback_file =
            RollbackManager::rollback_file_path(
                &paths,
            );

        let mut rollback_storage =
            RollbackStorage::new(
                &rollback_file,
            )
            .expect("rollback storage");

        let package =
            test_package(
                &root,
                "tool",
            );

        let install_path =
            package
                .install_path
                .as_ref()
                .expect(
                    "install path",
                );

        fs::create_dir_all(
            install_path
                .parent()
                .expect(
                    "install parent",
                ),
        )
        .expect(
            "create install parent",
        );

        fs::write(
            install_path,
            vec![
                1_u8;
                64 * 1024
            ],
        )
        .expect(
            "write install",
        );

        {
            let mut manager =
                RollbackManager::new(
                    &paths,
                    &mut package_database,
                    &mut rollback_storage,
                );

            manager
                .capture_from_installed(
                    &package,
                    RollbackSource::Upgrade,
                    &mut None::<fn(&str)>,
                )
                .expect(
                    "first capture",
                );
        }

        let manager =
            RollbackManager::new(
                &paths,
                &mut package_database,
                &mut rollback_storage,
            );

        let impact = manager
            .estimate_capture_impact(
                &package,
            )
            .expect(
                "capture impact",
            );

        assert!(
            impact.bytes.is_some()
        );

        assert_eq!(
            impact.confidence,
            SizeConfidence::Estimated
        );

        cleanup(&root)
            .expect("cleanup");
    }
}
