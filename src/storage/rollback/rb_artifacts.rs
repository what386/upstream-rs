use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use flate2::read::GzDecoder;
use tar::{Archive, Builder};
use walkdir::WalkDir;
use zstd::{Decoder as ZstdDecoder, Encoder as ZstdEncoder};

use crate::models::common::enums::CompressionLevel;
use crate::models::upstream::Package;
use crate::utils::filesystem::safe_move;

use super::rb_index::{RollbackArtifactFormat, RollbackRecord, RollbackSource};

pub(crate) struct CreatedRollback {
    pub(crate) record: RollbackRecord,
    pub(crate) artifact_path: PathBuf,
}

pub(crate) struct PreparedRollback {
    artifact_source_path: PathBuf,
    icon_source_path: Option<PathBuf>,
    extraction_dir: Option<PathBuf>,
}

impl PreparedRollback {
    pub(crate) fn artifact_source_path(&self) -> &Path {
        &self.artifact_source_path
    }

    pub(crate) fn icon_source_path(&self) -> Option<&Path> {
        self.icon_source_path.as_deref()
    }
}

impl Drop for PreparedRollback {
    fn drop(&mut self) {
        if let Some(extraction_dir) = self.extraction_dir.take() {
            let _ = remove_file_or_dir_if_exists(&extraction_dir);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArtifactSizeEstimate {
    pub(crate) bytes: u64,
    pub(crate) exact: bool,
}

pub(crate) struct RollbackArtifacts<'a> {
    root: &'a Path,
}

impl<'a> RollbackArtifacts<'a> {
    pub(crate) fn new(root: &'a Path) -> Self {
        Self { root }
    }

    pub(crate) fn create(
        &self,
        package: &Package,
        artifact_path: &Path,
        icon_source: Option<&Path>,
        source: RollbackSource,
        compression_level: CompressionLevel,
    ) -> Result<CreatedRollback> {
        let artifact_name = artifact_path.file_name().ok_or_else(|| {
            anyhow!(
                "Rollback artifact path '{}' has no final file name",
                artifact_path.display()
            )
        })?;

        let package_rollback_dir = safe_package_dir(self.root, &package.id)?;
        fs::create_dir_all(&package_rollback_dir).with_context(|| {
            format!(
                "Failed to create rollback directory '{}'",
                package_rollback_dir.display()
            )
        })?;

        let capture_id = rollback_capture_id(&source);
        let capture_dir = package_rollback_dir.join(&capture_id);
        fs::create_dir_all(&capture_dir).with_context(|| {
            format!(
                "Failed to create rollback capture directory '{}'",
                capture_dir.display()
            )
        })?;

        let artifact_entry_path = PathBuf::from("artifact").join(artifact_name);
        let rollback_artifact = capture_dir.join(&artifact_entry_path);

        if let Some(parent) = rollback_artifact.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create rollback artifact parent '{}'",
                    parent.display()
                )
            })?;
        }

        let archive_path = package_rollback_dir.join(format!("{capture_id}.tar.zst"));

        let capture_result = (|| {
            safe_move::copy_file_or_dir(artifact_path, &rollback_artifact)?;

            let icon_entry_path = capture_icon(self.root, icon_source, &capture_dir)?;
            let created_at = Utc::now();

            if matches!(compression_level, CompressionLevel::None) {
                return Ok(CreatedRollback {
                    record: RollbackRecord {
                        package_snapshot: package.clone(),
                        artifact_relative_path: path_relative_to(self.root, &rollback_artifact)?,
                        icon_relative_path: icon_entry_path
                            .as_ref()
                            .map(|entry| path_relative_to(self.root, &capture_dir.join(entry)))
                            .transpose()?,
                        artifact_format: RollbackArtifactFormat::Raw,
                        artifact_entry_path: None,
                        icon_entry_path: None,
                        source,
                        created_at,
                    },
                    artifact_path: rollback_artifact.clone(),
                });
            }

            compress_capture_dir(&capture_dir, &archive_path, compression_level)?;

            fs::remove_dir_all(&capture_dir).with_context(|| {
                format!(
                    "Failed to remove rollback staging directory '{}'",
                    capture_dir.display()
                )
            })?;

            Ok(CreatedRollback {
                record: RollbackRecord {
                    package_snapshot: package.clone(),
                    artifact_relative_path: path_relative_to(self.root, &archive_path)?,
                    icon_relative_path: None,
                    artifact_format: RollbackArtifactFormat::TarZstd,
                    artifact_entry_path: Some(artifact_entry_path),
                    icon_entry_path,
                    source,
                    created_at,
                },
                artifact_path: archive_path.clone(),
            })
        })();

        match capture_result {
            Ok(created) => Ok(created),
            Err(capture_error) => {
                let cleanup_result = remove_file_or_dir_if_exists(&capture_dir)
                    .and_then(|()| remove_file_or_dir_if_exists(&archive_path))
                    .and_then(|()| self.cleanup_empty_package_dir(&package.id));

                match cleanup_result {
                    Ok(()) => Err(capture_error),
                    Err(cleanup_error) => Err(anyhow!(
                        "{capture_error:#}. Failed to clean incomplete rollback capture: {cleanup_error:#}"
                    )),
                }
            }
        }
    }

    pub(crate) fn prepare(
        &self,
        package_name: &str,
        record: &RollbackRecord,
    ) -> Result<PreparedRollback> {
        match record.artifact_format {
            RollbackArtifactFormat::Raw => Ok(PreparedRollback {
                artifact_source_path: self.root.join(&record.artifact_relative_path),
                icon_source_path: record
                    .icon_relative_path
                    .as_ref()
                    .map(|path| self.root.join(path)),
                extraction_dir: None,
            }),

            RollbackArtifactFormat::Tgz | RollbackArtifactFormat::TarZstd => {
                let extraction_dir = self.extract_archive(package_name, record)?;

                let artifact_entry = record.artifact_entry_path.as_ref().ok_or_else(|| {
                    anyhow!("Rollback archive record is missing artifact entry path")
                })?;

                Ok(PreparedRollback {
                    artifact_source_path: extraction_dir.join(artifact_entry),
                    icon_source_path: record
                        .icon_entry_path
                        .as_ref()
                        .map(|entry| extraction_dir.join(entry)),
                    extraction_dir: Some(extraction_dir),
                })
            }
        }
    }

    pub(crate) fn delete(&self, package_name: &str, record: &RollbackRecord) -> Result<()> {
        let package_dir = safe_package_dir(self.root, package_name)?;

        match record.artifact_format {
            RollbackArtifactFormat::Raw => {
                let artifact_path = self.root.join(&record.artifact_relative_path);
                remove_file_or_dir_if_exists(&artifact_path)?;

                if let Some(icon_path) = record.icon_relative_path.as_ref() {
                    let icon_path = self.root.join(icon_path);
                    remove_file_or_dir_if_exists(&icon_path)?;
                    cleanup_empty_rollback_ancestors(&package_dir, icon_path.parent())?;
                }

                cleanup_empty_rollback_ancestors(&package_dir, artifact_path.parent())?;
            }

            RollbackArtifactFormat::Tgz | RollbackArtifactFormat::TarZstd => {
                remove_file_or_dir_if_exists(&self.root.join(&record.artifact_relative_path))?;
            }
        }

        self.cleanup_empty_package_dir(package_name)
    }

    pub(crate) fn rollback_record_size(&self, record: &RollbackRecord) -> Result<u64> {
        let artifact_path = self.root.join(&record.artifact_relative_path);
        let mut total = estimate_path_size(&artifact_path)?;

        if matches!(record.artifact_format, RollbackArtifactFormat::Raw)
            && let Some(icon_path) = record.icon_relative_path.as_ref()
        {
            total = total
                .checked_add(estimate_path_size(&self.root.join(icon_path))?)
                .ok_or_else(|| anyhow!("Rollback size overflow"))?;
        }

        Ok(total)
    }

    pub(crate) fn package_size(&self, package_name: &str) -> Result<u64> {
        estimate_path_size(&safe_package_dir(self.root, package_name)?)
    }

    pub(crate) fn restored_size(&self, record: &RollbackRecord) -> Result<ArtifactSizeEstimate> {
        match record.artifact_format {
            RollbackArtifactFormat::Raw => {
                let path = self.root.join(&record.artifact_relative_path);
                Ok(ArtifactSizeEstimate {
                    bytes: estimate_path_size(&path)?,
                    exact: true,
                })
            }

            RollbackArtifactFormat::Tgz | RollbackArtifactFormat::TarZstd => {
                let archive_path = self.root.join(&record.artifact_relative_path);
                let archive_file = File::open(&archive_path).with_context(|| {
                    format!(
                        "Failed to open rollback archive '{}'",
                        archive_path.display()
                    )
                })?;

                let decoder = compressed_archive_reader(&record.artifact_format, archive_file)?;
                let mut archive = Archive::new(decoder);
                let artifact_root = record
                    .artifact_entry_path
                    .as_deref()
                    .and_then(|path| path.components().next())
                    .map(|component| PathBuf::from(component.as_os_str()))
                    .unwrap_or_else(|| PathBuf::from("artifact"));

                let mut total = 0_u64;

                for entry in archive
                    .entries()
                    .context("Failed to read rollback archive entries")?
                {
                    let entry = entry.context("Failed to read rollback archive entry")?;
                    let path = entry
                        .path()
                        .context("Failed to read rollback archive entry path")?;

                    if path.starts_with(&artifact_root) && entry.header().entry_type().is_file() {
                        total = total
                            .checked_add(entry.header().size()?)
                            .ok_or_else(|| anyhow!("Restored size overflow"))?;
                    }
                }

                Ok(ArtifactSizeEstimate {
                    bytes: total,
                    exact: false,
                })
            }
        }
    }

    pub(crate) fn cleanup_empty_package_dir(&self, package_name: &str) -> Result<()> {
        let package_dir = safe_package_dir(self.root, package_name)?;

        if path_exists_no_follow(&package_dir)?
            && package_dir
                .read_dir()
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
        {
            fs::remove_dir(&package_dir).with_context(|| {
                format!(
                    "Failed to remove empty rollback directory '{}'",
                    package_dir.display()
                )
            })?;
        }

        Ok(())
    }

    fn extract_archive(&self, package_name: &str, record: &RollbackRecord) -> Result<PathBuf> {
        let archive_path = self.root.join(&record.artifact_relative_path);

        if !path_exists_no_follow(&archive_path)? {
            return Err(anyhow!(
                "Rollback archive is missing for '{}': {}",
                package_name,
                archive_path.display()
            ));
        }

        let package_component = safe_package_dir(self.root, package_name)?
            .file_name()
            .expect("safe rollback package directory has a name")
            .to_string_lossy()
            .into_owned();

        let extraction_id = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| Utc::now().timestamp_micros() * 1_000);

        let extract_dir = self.root.join(format!(
            ".restore-{}-{}-{}",
            package_component,
            std::process::id(),
            extraction_id
        ));

        fs::create_dir_all(&extract_dir).with_context(|| {
            format!(
                "Failed to create rollback extraction directory '{}'",
                extract_dir.display()
            )
        })?;

        let extract_result = (|| {
            let archive_file = File::open(&archive_path).with_context(|| {
                format!(
                    "Failed to open rollback archive '{}'",
                    archive_path.display()
                )
            })?;

            let decoder = compressed_archive_reader(&record.artifact_format, archive_file)?;
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

                let unpacked = entry.unpack_in(&extract_dir).with_context(|| {
                    format!(
                        "Failed to extract rollback archive entry '{}' into '{}'",
                        entry_path.display(),
                        extract_dir.display()
                    )
                })?;

                if !unpacked {
                    return Err(anyhow!(
                        "Rollback archive entry '{}' would escape extraction directory '{}'",
                        entry_path.display(),
                        extract_dir.display()
                    ));
                }
            }

            Ok(())
        })();

        if let Err(extract_error) = extract_result {
            return match remove_file_or_dir_if_exists(&extract_dir) {
                Ok(()) => Err(extract_error),
                Err(cleanup_error) => Err(anyhow!(
                    "{extract_error:#}. Failed to clean rollback extraction directory: {cleanup_error:#}"
                )),
            };
        }

        Ok(extract_dir)
    }
}

fn safe_package_dir(root: &Path, package_name: &str) -> Result<PathBuf> {
    if package_name.is_empty() {
        return Err(anyhow!(
            "Package name '{}' is not safe for rollback storage",
            package_name
        ));
    }

    // Package IDs are canonical references and may contain ':' and '/'. Keep
    // them as one filesystem component while retaining a reversible,
    // collision-free mapping for rollback artifacts.
    let encoded = package_component(package_name);

    Ok(root.join(encoded))
}

fn package_component(package_name: &str) -> String {
    package_name
        .bytes()
        .fold(String::new(), |mut encoded, byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
                encoded.push(byte as char);
            } else {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }

            encoded
        })
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
    root: &Path,
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
        .ok_or_else(|| anyhow!("Icon path '{}' has no file name", icon_path.display()))?;

    let icon_entry_path =
        PathBuf::from("icon").join(format!("icon-{}", icon_name.to_string_lossy()));

    let icon_backup = capture_dir.join(&icon_entry_path);

    if let Some(parent) = icon_backup.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create rollback icon parent '{}'",
                parent.display()
            )
        })?;
    }

    fs::copy(icon_path, &icon_backup).with_context(|| {
        format!(
            "Failed to copy icon '{}' to '{}'",
            icon_path.display(),
            icon_backup.display()
        )
    })?;

    path_relative_to(root, &icon_backup)?;
    Ok(Some(icon_entry_path))
}

fn zstd_level(level: CompressionLevel) -> Result<i32> {
    match level {
        CompressionLevel::None => Err(anyhow!(
            "Cannot create a compressed rollback archive with compression disabled"
        )),
        CompressionLevel::Low => Ok(1),
        CompressionLevel::High => Ok(9),
    }
}

fn compressed_archive_reader(
    format: &RollbackArtifactFormat,
    archive_file: File,
) -> Result<Box<dyn Read>> {
    match format {
        RollbackArtifactFormat::Tgz => Ok(Box::new(GzDecoder::new(archive_file))),
        RollbackArtifactFormat::TarZstd => Ok(Box::new(
            ZstdDecoder::new(archive_file).context("Failed to initialize rollback zstd decoder")?,
        )),
        RollbackArtifactFormat::Raw => Err(anyhow!(
            "Raw rollback artifact cannot be opened as a compressed archive"
        )),
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

    let encoder = ZstdEncoder::new(archive_file, zstd_level(level)?)
        .context("Failed to initialize rollback zstd encoder")?;

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
        .context("Failed to finish rollback zstd archive")?;

    Ok(())
}

fn append_capture_entry<W: Write>(
    builder: &mut Builder<W>,
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

fn path_relative_to(base: &Path, full: &Path) -> Result<PathBuf> {
    full.strip_prefix(base).map(Path::to_path_buf).map_err(|_| {
        anyhow!(
            "Path '{}' is not under '{}'",
            full.display(),
            base.display()
        )
    })
}

fn is_safe_archive_entry(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
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

fn remove_file_or_dir_if_exists(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to inspect path '{}'", path.display()));
        }
    };

    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory '{}'", path.display()))?;
    } else {
        fs::remove_file(path)
            .with_context(|| format!("Failed to remove file '{}'", path.display()))?;
    }

    Ok(())
}

fn path_exists_no_follow(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to inspect path '{}'", path.display()))
        }
    }
}

fn estimate_path_size(path: &Path) -> Result<u64> {
    if !path_exists_no_follow(path)? {
        return Ok(0);
    }

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("Failed to inspect path '{}'", path.display()))?;

    if metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(metadata.len());
    }

    let mut total = 0_u64;
    for entry in WalkDir::new(path).follow_links(false) {
        crate::application::cancellation::check()?;
        let entry = entry.with_context(|| format!("Failed to walk path '{}'", path.display()))?;
        let metadata = fs::symlink_metadata(entry.path())
            .with_context(|| format!("Failed to inspect path '{}'", entry.path().display()))?;

        if metadata.is_file() || metadata.file_type().is_symlink() {
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| anyhow!("Rollback size overflow"))?;
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Read;

    use flate2::{Compression, write::GzEncoder};
    use tar::{Archive, Builder, Header};

    use crate::storage::rollback::RollbackArtifactFormat;
    use crate::utils::test_support;

    use super::{compressed_archive_reader, safe_package_dir};

    #[test]
    fn canonical_package_ids_use_one_safe_storage_component() {
        let root = test_support::temp_root("upstream-rollback-artifacts-test", "canonical-id");
        let path = safe_package_dir(&root, "github:JustVugg/colibri").expect("safe path");

        assert_eq!(path, root.join("github%3AJustVugg%2Fcolibri"));
        assert_eq!(path.parent(), Some(root.as_path()));

        fs::create_dir_all(&root).expect("create test root");
        fs::remove_dir_all(root).expect("cleanup test root");
    }

    #[test]
    fn legacy_tgz_reader_remains_supported() {
        let root = test_support::temp_root("upstream-rollback-artifacts-test", "legacy-tgz");
        fs::create_dir_all(&root).expect("create test root");
        let archive_path = root.join("legacy.tgz");

        let archive_file = File::create(&archive_path).expect("create legacy archive");
        let encoder = GzEncoder::new(archive_file, Compression::fast());
        let mut builder = Builder::new(encoder);
        let contents = b"legacy rollback";
        let mut header = Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "artifact/tool", &contents[..])
            .expect("append legacy artifact");
        let encoder = builder.into_inner().expect("finish legacy tar");
        encoder.finish().expect("finish legacy gzip");

        let restored = {
            let archive_file = File::open(&archive_path).expect("open legacy archive");
            let decoder = compressed_archive_reader(&RollbackArtifactFormat::Tgz, archive_file)
                .expect("open legacy decoder");

            let mut archive = Archive::new(decoder);
            let mut entries = archive.entries().expect("read legacy entries");
            let mut entry = entries
                .next()
                .expect("legacy artifact entry")
                .expect("read legacy artifact entry");

            let mut restored = Vec::new();
            entry
                .read_to_end(&mut restored)
                .expect("read legacy artifact");
            restored
        };

        assert_eq!(restored, contents);

        fs::remove_dir_all(root).expect("cleanup test root");
    }
}
