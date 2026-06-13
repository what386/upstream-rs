use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use tar::{Archive, Builder};

use crate::models::common::enums::CompressionLevel;
use crate::models::upstream::Package;
use crate::models::upstream::app_config::RollbackConfig;
use crate::services::packaging::PackageRemover;
use crate::services::packaging::disk_impact::{
    ByteEstimate, DiskImpact, SignedByteEstimate, estimate_path_size,
};
use crate::services::storage::{
    config_storage::ConfigStorage,
    metadata_storage::MetadataStorage,
    package_storage::PackageStorage,
    rollback_storage::{RollbackArtifactFormat, RollbackRecord, RollbackSource, RollbackStorage},
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

pub struct RollbackManager<'a> {
    paths: &'a UpstreamPaths,
    package_storage: &'a mut PackageStorage,
    metadata_storage: &'a mut MetadataStorage,
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

    fn capture_options(paths: &UpstreamPaths) -> Result<RollbackCaptureOptions> {
        let config = ConfigStorage::new(&paths.config.config_file)?;
        let rollback = &config.get_config().rollback;
        Ok(RollbackCaptureOptions {
            compression_level: rollback.compression_level,
            stored_artifacts: effective_stored_artifacts(rollback),
        })
    }

    pub fn new(
        paths: &'a UpstreamPaths,
        package_storage: &'a mut PackageStorage,
        metadata_storage: &'a mut MetadataStorage,
        rollback_storage: &'a mut RollbackStorage,
    ) -> Self {
        Self {
            paths,
            package_storage,
            metadata_storage,
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

        let options = Self::capture_options(self.paths)?;
        let record = Self::capture_artifact_from_path(
            self.paths,
            package,
            install_path,
            source,
            options,
            message_callback,
        )?;
        let pruned =
            self.rollback_storage
                .push_record(&package.name, record, options.stored_artifacts)?;
        for record in pruned {
            delete_record_artifacts(self.paths, &package.name, &record)?;
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
        let options = Self::capture_options(paths)?;
        let record = Self::capture_artifact_from_path(
            paths,
            package,
            backup_path,
            source,
            options,
            message_callback,
        )?;
        let pruned =
            rollback_storage.push_record(&package.name, record, options.stored_artifacts)?;
        for record in pruned {
            delete_record_artifacts(paths, &package.name, &record)?;
        }
        Ok(())
    }

    fn capture_artifact_from_path<H>(
        paths: &UpstreamPaths,
        package: &Package,
        artifact_path: &Path,
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
        let package_rollback_dir = paths.install.rollback_dir.join(&package.name);
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
        safe_move::move_file_or_dir(artifact_path, &rollback_artifact)?;

        let icon_entry_path = capture_icon(paths, package, &capture_dir)?;

        let created_at = Utc::now();
        if matches!(options.compression_level, CompressionLevel::None) {
            return Ok(RollbackRecord {
                package_snapshot: package.clone(),
                artifact_relative_path: path_relative_to(
                    &paths.install.rollback_dir,
                    &rollback_artifact,
                )?,
                icon_relative_path: icon_entry_path
                    .as_ref()
                    .map(|entry| {
                        path_relative_to(&paths.install.rollback_dir, &capture_dir.join(entry))
                    })
                    .transpose()?,
                artifact_format: RollbackArtifactFormat::Raw,
                artifact_entry_path: None,
                icon_entry_path: None,
                source,
                created_at,
            });
        }

        let archive_path = package_rollback_dir.join(format!("{capture_id}.tgz"));
        compress_capture_dir(&capture_dir, &archive_path, options.compression_level)?;
        fs::remove_dir_all(&capture_dir).context(format!(
            "Failed to remove rollback staging directory '{}'",
            capture_dir.display()
        ))?;

        Ok(RollbackRecord {
            package_snapshot: package.clone(),
            artifact_relative_path: path_relative_to(&paths.install.rollback_dir, &archive_path)?,
            icon_relative_path: None,
            artifact_format: RollbackArtifactFormat::Tgz,
            artifact_entry_path: Some(artifact_entry_path),
            icon_entry_path,
            source,
            created_at,
        })
    }

    pub fn restore_package<H>(
        &mut self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let Some(record) = self.rollback_storage.get_record(package_name).cloned() else {
            return Err(anyhow!("No rollback data found for '{}'", package_name));
        };

        if let Some(current) = self
            .package_storage
            .get_package_by_name(package_name)
            .cloned()
        {
            message!(
                message_callback,
                "Removing current installation for '{}' before rollback ...",
                package_name
            );
            let remover = PackageRemover::new(self.paths);
            remover.remove_package_files(&current, message_callback)?;
            self.package_storage.remove_package_by_name(package_name)?;
            self.metadata_storage.remove_package(package_name)?;
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
                Some(extract_record_archive(self.paths, package_name, &record)?)
            }
        };
        let source_path =
            record_artifact_source_path(self.paths, &record, extracted_dir.as_deref())?;
        if !source_path.exists() {
            return Err(anyhow!(
                "Rollback artifact is missing for '{}': {}",
                package_name,
                source_path.display()
            ));
        }

        safe_move::move_file_or_dir(&source_path, &target_install_path)?;

        let icon_source = record_icon_source_path(self.paths, &record, extracted_dir.as_deref())?;
        if let (Some(icon_source), Some(icon_target)) = (
            icon_source.as_ref(),
            record.package_snapshot.icon_path.as_ref(),
        ) {
            if icon_source.exists() {
                if let Some(parent) = icon_target.parent() {
                    fs::create_dir_all(parent).context(format!(
                        "Failed to create icon parent '{}'",
                        parent.display()
                    ))?;
                }
                fs::copy(&icon_source, icon_target).context(format!(
                    "Failed to restore icon from '{}' to '{}'",
                    icon_source.display(),
                    icon_target.display()
                ))?;
            }
        }

        self.package_storage
            .add_or_update_package(record.package_snapshot.clone())?;
        let remover = PackageRemover::new(self.paths);
        remover.restore_runtime_integrations(&record.package_snapshot, message_callback)?;

        self.rollback_storage.remove_record(package_name)?;
        delete_record_artifacts(self.paths, package_name, &record)?;
        if let Some(extracted_dir) = extracted_dir
            && extracted_dir.exists()
        {
            let _ = fs::remove_dir_all(extracted_dir);
        }

        Ok(())
    }

    pub fn prune_package(&mut self, package_name: &str) -> Result<bool> {
        let removed = self.rollback_storage.remove_all_records(package_name)?;
        for record in &removed {
            delete_record_artifacts(self.paths, package_name, record)?;
        }
        cleanup_empty_package_rollback_dir(self.paths, package_name)?;
        Ok(!removed.is_empty())
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

    pub fn estimate_restore_impact(&self, package_name: &str) -> Option<DiskImpact> {
        self.rollback_storage.get_record(package_name)?;
        let current_size = self
            .package_storage
            .get_package_by_name(package_name)
            .map(|package| {
                PackageRemover::new(self.paths)
                    .estimate_active_size(package)
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        Some(DiskImpact {
            download: ByteEstimate::exact(0),
            net: SignedByteEstimate::exact(-i128::from(current_size)),
        })
    }

    pub fn estimate_prune_impact(&self, package_name: &str) -> Option<DiskImpact> {
        self.rollback_storage.get_record(package_name)?;
        let rollback_dir_size =
            estimate_path_size(&self.paths.install.rollback_dir.join(package_name)).unwrap_or(0);

        Some(DiskImpact {
            download: ByteEstimate::exact(0),
            net: SignedByteEstimate::exact(-i128::from(rollback_dir_size)),
        })
    }
}

fn path_relative_to(base: &Path, full: &Path) -> Result<PathBuf> {
    full.strip_prefix(base).map(Path::to_path_buf).map_err(|_| {
        anyhow!(
            "Path '{}' is not under '{}'",
            full.display(),
            base.display()
        )
    })
}

fn effective_stored_artifacts(config: &RollbackConfig) -> usize {
    config.stored_artifacts.max(1) as usize
}

fn rollback_capture_id(source: &RollbackSource) -> String {
    let source_label = match source {
        RollbackSource::Upgrade => "upgrade",
        RollbackSource::Reinstall => "reinstall",
        RollbackSource::Remove => "remove",
    };
    let timestamp = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_micros() * 1_000);
    format!("{timestamp}-{source_label}")
}

fn capture_icon(
    paths: &UpstreamPaths,
    package: &Package,
    capture_dir: &Path,
) -> Result<Option<PathBuf>> {
    let Some(icon_path) = package.icon_path.as_ref() else {
        return Ok(None);
    };
    if !icon_path.exists() {
        return Ok(None);
    }

    let icon_name = icon_path
        .file_name()
        .ok_or_else(|| anyhow!("Icon path '{}' has no file name", icon_path.display()))?;
    let icon_entry_path =
        PathBuf::from("icon").join(format!("icon-{}", icon_name.to_string_lossy()));
    let icon_backup = capture_dir.join(&icon_entry_path);
    if let Some(parent) = icon_backup.parent() {
        fs::create_dir_all(parent).context(format!(
            "Failed to create rollback icon parent '{}'",
            parent.display()
        ))?;
    }
    fs::copy(icon_path, &icon_backup).context(format!(
        "Failed to copy icon '{}' to '{}'",
        icon_path.display(),
        icon_backup.display()
    ))?;

    path_relative_to(&paths.install.rollback_dir, &icon_backup)?;
    Ok(Some(icon_entry_path))
}

fn gzip_level(level: CompressionLevel) -> Compression {
    match level {
        CompressionLevel::None => Compression::none(),
        CompressionLevel::Low => Compression::fast(),
        CompressionLevel::High => Compression::best(),
    }
}

fn compress_capture_dir(
    capture_dir: &Path,
    archive_path: &Path,
    level: CompressionLevel,
) -> Result<()> {
    let archive_file = File::create(archive_path).with_context(|| {
        format!(
            "Failed to create rollback archive '{}'",
            archive_path.display()
        )
    })?;
    let encoder = GzEncoder::new(archive_file, gzip_level(level));
    let mut builder = Builder::new(encoder);

    append_capture_entry(&mut builder, capture_dir, Path::new("artifact"))?;
    let icon_dir = capture_dir.join("icon");
    if icon_dir.exists() {
        append_capture_entry(&mut builder, capture_dir, Path::new("icon"))?;
    }

    let encoder = builder
        .into_inner()
        .context("Failed to finish rollback tar archive")?;
    encoder
        .finish()
        .context("Failed to finish rollback gzip archive")?;
    Ok(())
}

fn append_capture_entry(
    builder: &mut Builder<GzEncoder<File>>,
    capture_dir: &Path,
    entry: &Path,
) -> Result<()> {
    let full_path = capture_dir.join(entry);
    if full_path.is_dir() {
        builder
            .append_dir_all(entry, &full_path)
            .with_context(|| format!("Failed to archive '{}'", full_path.display()))?;
    } else if full_path.is_file() {
        builder
            .append_path_with_name(&full_path, entry)
            .with_context(|| format!("Failed to archive '{}'", full_path.display()))?;
    }
    Ok(())
}

fn extract_record_archive(
    paths: &UpstreamPaths,
    package_name: &str,
    record: &RollbackRecord,
) -> Result<PathBuf> {
    let archive_path = paths
        .install
        .rollback_dir
        .join(&record.artifact_relative_path);
    if !archive_path.exists() {
        return Err(anyhow!(
            "Rollback archive is missing for '{}': {}",
            package_name,
            archive_path.display()
        ));
    }

    let extract_dir = paths.install.rollback_dir.join(format!(
        ".restore-{}-{}",
        package_name,
        std::process::id()
    ));
    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir).context(format!(
            "Failed to clear rollback extraction directory '{}'",
            extract_dir.display()
        ))?;
    }
    fs::create_dir_all(&extract_dir).context(format!(
        "Failed to create rollback extraction directory '{}'",
        extract_dir.display()
    ))?;

    let archive_file = File::open(&archive_path).with_context(|| {
        format!(
            "Failed to open rollback archive '{}'",
            archive_path.display()
        )
    })?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);
    for entry in archive
        .entries()
        .context("Failed to read rollback archive entries")?
    {
        let mut entry = entry.context("Failed to read rollback archive entry")?;
        let entry_path = entry
            .path()
            .context("Failed to read rollback archive entry path")?
            .into_owned();
        if !is_safe_archive_entry(&entry_path) {
            return Err(anyhow!(
                "Rollback archive contains unsafe path '{}'",
                entry_path.display()
            ));
        }
        entry.unpack_in(&extract_dir).with_context(|| {
            format!(
                "Failed to extract rollback archive entry '{}' into '{}'",
                entry_path.display(),
                extract_dir.display()
            )
        })?;
    }

    Ok(extract_dir)
}

fn is_safe_archive_entry(path: &Path) -> bool {
    path.is_relative()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn record_artifact_source_path(
    paths: &UpstreamPaths,
    record: &RollbackRecord,
    extracted_dir: Option<&Path>,
) -> Result<PathBuf> {
    match record.artifact_format {
        RollbackArtifactFormat::Raw => Ok(paths
            .install
            .rollback_dir
            .join(&record.artifact_relative_path)),
        RollbackArtifactFormat::Tgz => {
            let extract_dir =
                extracted_dir.ok_or_else(|| anyhow!("Rollback archive was not extracted"))?;
            let entry = record
                .artifact_entry_path
                .as_ref()
                .ok_or_else(|| anyhow!("Rollback archive record is missing artifact entry path"))?;
            Ok(extract_dir.join(entry))
        }
    }
}

fn record_icon_source_path(
    paths: &UpstreamPaths,
    record: &RollbackRecord,
    extracted_dir: Option<&Path>,
) -> Result<Option<PathBuf>> {
    match record.artifact_format {
        RollbackArtifactFormat::Raw => Ok(record
            .icon_relative_path
            .as_ref()
            .map(|path| paths.install.rollback_dir.join(path))),
        RollbackArtifactFormat::Tgz => {
            let Some(entry) = record.icon_entry_path.as_ref() else {
                return Ok(None);
            };
            let extract_dir =
                extracted_dir.ok_or_else(|| anyhow!("Rollback archive was not extracted"))?;
            Ok(Some(extract_dir.join(entry)))
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
                .install
                .rollback_dir
                .join(&record.artifact_relative_path);
            remove_file_or_dir_if_exists(&artifact_path)?;
            if let Some(icon_path) = record.icon_relative_path.as_ref() {
                let icon_path = paths.install.rollback_dir.join(icon_path);
                remove_file_or_dir_if_exists(&icon_path)?;
                cleanup_empty_rollback_ancestors(
                    &paths.install.rollback_dir.join(package_name),
                    icon_path.parent(),
                )?;
            }
            cleanup_empty_rollback_ancestors(
                &paths.install.rollback_dir.join(package_name),
                artifact_path.parent(),
            )?;
        }
        RollbackArtifactFormat::Tgz => {
            remove_file_or_dir_if_exists(
                &paths
                    .install
                    .rollback_dir
                    .join(&record.artifact_relative_path),
            )?;
        }
    }
    cleanup_empty_package_rollback_dir(paths, package_name)
}

fn cleanup_empty_rollback_ancestors(package_dir: &Path, start: Option<&Path>) -> Result<()> {
    let Some(mut current) = start else {
        return Ok(());
    };
    while current.starts_with(package_dir) && current != package_dir {
        if current.exists()
            && current
                .read_dir()
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
        {
            fs::remove_dir(current).with_context(|| {
                format!(
                    "Failed to remove empty rollback directory '{}'",
                    current.display()
                )
            })?;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }
    Ok(())
}

fn cleanup_empty_package_rollback_dir(paths: &UpstreamPaths, package_name: &str) -> Result<()> {
    let package_dir = paths.install.rollback_dir.join(package_name);
    if package_dir.exists()
        && package_dir
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    {
        fs::remove_dir(&package_dir).context(format!(
            "Failed to remove empty rollback directory '{}'",
            package_dir.display()
        ))?;
    }
    Ok(())
}

fn remove_file_or_dir_if_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory '{}'", path.display()))?;
    } else if path.is_file() {
        fs::remove_file(path)
            .with_context(|| format!("Failed to remove file '{}'", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RollbackManager;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::services::storage::rollback_storage::{RollbackArtifactFormat, RollbackSource};
    use crate::services::storage::{
        metadata_storage::MetadataStorage, package_storage::PackageStorage,
        rollback_storage::RollbackStorage,
    };
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

    fn write_rollback_config(root: &Path, compression_level: &str, stored_artifacts: u32) {
        let paths = test_support::upstream_paths(root);
        fs::create_dir_all(paths.config.config_file.parent().expect("config parent"))
            .expect("create config parent");
        fs::write(
            &paths.config.config_file,
            format!(
                "[rollback]\ncompression_level = \"{compression_level}\"\nstored_artifacts = {stored_artifacts}\n"
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
    fn capture_from_installed_retains_multiple_compressed_artifacts() {
        let root = temp_root("compressed-retention");
        write_rollback_config(&root, "low", 2);
        let paths = test_support::upstream_paths(&root);
        let mut package_storage =
            PackageStorage::new(&paths.config.packages_file).expect("package storage");
        let mut metadata_storage =
            MetadataStorage::new(&paths.config.metadata_file).expect("metadata storage");
        let rollback_file = RollbackManager::rollback_file_path(&paths);
        let mut rollback_storage = RollbackStorage::new(&rollback_file).expect("rollback storage");
        let package = test_package(&root, "tool");
        let install_path = package.install_path.as_ref().expect("install path");
        fs::create_dir_all(install_path.parent().expect("install parent"))
            .expect("create install parent");

        {
            let mut manager = RollbackManager::new(
                &paths,
                &mut package_storage,
                &mut metadata_storage,
                &mut rollback_storage,
            );
            for contents in ["one", "two", "three"] {
                fs::write(install_path, contents).expect("write install artifact");
                manager
                    .capture_from_installed(
                        &package,
                        RollbackSource::Upgrade,
                        &mut None::<fn(&str)>,
                    )
                    .expect("capture rollback");
            }
        }

        let records = rollback_storage.get_records("tool");
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.artifact_format == RollbackArtifactFormat::Tgz)
        );
        assert!(records.iter().all(|record| {
            record
                .artifact_relative_path
                .extension()
                .is_some_and(|extension| extension == "tgz")
        }));
        assert_eq!(
            fs::read_dir(paths.install.rollback_dir.join("tool"))
                .expect("rollback package dir")
                .count(),
            2
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn restore_package_decompresses_tgz_artifact() {
        let root = temp_root("compressed-restore");
        write_rollback_config(&root, "high", 1);
        let paths = test_support::upstream_paths(&root);
        let mut package_storage =
            PackageStorage::new(&paths.config.packages_file).expect("package storage");
        let mut metadata_storage =
            MetadataStorage::new(&paths.config.metadata_file).expect("metadata storage");
        let rollback_file = RollbackManager::rollback_file_path(&paths);
        let mut rollback_storage = RollbackStorage::new(&rollback_file).expect("rollback storage");
        let package = test_package(&root, "tool");
        let install_path = package.install_path.as_ref().expect("install path").clone();
        fs::create_dir_all(install_path.parent().expect("install parent"))
            .expect("create install parent");
        fs::write(&install_path, "before-upgrade").expect("write install artifact");

        {
            let mut manager = RollbackManager::new(
                &paths,
                &mut package_storage,
                &mut metadata_storage,
                &mut rollback_storage,
            );
            manager
                .capture_from_installed(&package, RollbackSource::Upgrade, &mut None::<fn(&str)>)
                .expect("capture rollback");
            assert!(!install_path.exists());
            assert_eq!(
                manager
                    .rollback_record("tool")
                    .expect("record")
                    .artifact_format,
                RollbackArtifactFormat::Tgz
            );

            manager
                .restore_package("tool", &mut None::<fn(&str)>)
                .expect("restore rollback");
        }

        assert_eq!(
            fs::read_to_string(&install_path).expect("restored artifact"),
            "before-upgrade"
        );
        assert!(rollback_storage.get_record("tool").is_none());

        cleanup(&root).expect("cleanup");
    }
}
