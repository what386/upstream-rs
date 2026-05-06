use crate::{
    models::upstream::PackageReference,
    services::{
        storage::{config_storage::ConfigStorage, package_storage::PackageStorage},
        trust::MinisignPublicKey,
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow, bail};
use console::style;
use minisign_verify::PublicKey;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::Path};

// ---------------------------------------------------------------------------
// Manifest (mirrors ExportManifest but only needs Deserialize)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ImportManifest {
    pub version: u32,
    pub packages: Vec<PackageReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Keys,
    Manifest,
    Snapshot,
}

/// Returns true if the path looks like a tarball we produced.
fn is_snapshot(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name.ends_with(".tar.gz") || name.ends_with(".tgz")
}

// ---------------------------------------------------------------------------
// Operation
// ---------------------------------------------------------------------------

pub struct ImportOperation<'a> {
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
}

impl<'a> ImportOperation<'a> {
    pub fn new(package_storage: &'a mut PackageStorage, paths: &'a UpstreamPaths) -> Self {
        Self {
            package_storage,
            paths,
        }
    }

    pub fn detect_kind(path: &Path, forced_kind: Option<ImportKind>) -> Result<ImportKind> {
        if let Some(kind) = forced_kind {
            return Ok(kind);
        }

        if is_snapshot(path) {
            return Ok(ImportKind::Snapshot);
        }

        if Self::read_manifest(path).is_ok() {
            return Ok(ImportKind::Manifest);
        }

        if Self::parse_minisign_key_file(path).is_ok() {
            return Ok(ImportKind::Keys);
        }

        Err(anyhow!(
            "Could not detect import type for '{}'. Use --as keys|manifest|snapshot.",
            path.display()
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn import<F, G, H>(
        &mut self,
        path: &Path,
        skip_failed: bool,
        forced_kind: Option<ImportKind>,
        _yes: bool,
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let kind = Self::detect_kind(path, forced_kind)?;

        match kind {
            ImportKind::Snapshot => {
                if skip_failed {
                    message!(
                        message_callback,
                        "{}",
                        style("Note: --skip-failed has no effect for snapshot imports").yellow()
                    );
                }
                self.import_snapshot(path, message_callback)
            }
            ImportKind::Manifest => {
                let manifest = Self::read_manifest(path)?;
                self.import_manifest_metadata(
                    manifest,
                    skip_failed,
                    download_progress_callback,
                    overall_progress_callback,
                    message_callback,
                )
                .await
            }
            ImportKind::Keys => {
                let keys = Self::parse_minisign_key_file(path)?;
                self.import_keys(keys, skip_failed, message_callback)
            }
        }
    }

    fn read_manifest(path: &Path) -> Result<ImportManifest> {
        let content = fs::read_to_string(path)
            .context(format!("Failed to read manifest from '{}'", path.display()))?;
        let manifest: ImportManifest =
            serde_json::from_str(&content).context("Failed to parse manifest")?;
        if manifest.version != 1 {
            bail!(
                "Unsupported manifest version {}. Upgrade upstream and try again.",
                manifest.version
            );
        }
        Ok(manifest)
    }

    async fn import_manifest_metadata<F, G, H>(
        &mut self,
        manifest: ImportManifest,
        skip_failed: bool,
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let _ = download_progress_callback;

        let total = manifest.packages.len() as u32;
        let mut completed = 0_u32;
        let mut imported = 0_u32;
        let mut skipped = 0_u32;

        for reference in manifest.packages {
            if self
                .package_storage
                .get_package_by_name(&reference.name)
                .is_some()
            {
                skipped += 1;
                message!(
                    message_callback,
                    "{} Package '{}' already exists; skipping",
                    style("Skipped:").yellow(),
                    reference.name
                );
            } else if let Err(err) = self
                .package_storage
                .add_or_update_package(reference.into_package())
            {
                if skip_failed {
                    skipped += 1;
                    message!(
                        message_callback,
                        "{} {}",
                        style("Failed to import package metadata:").red(),
                        err
                    );
                } else {
                    return Err(err);
                }
            } else {
                imported += 1;
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        message!(
            message_callback,
            "Manifest import complete: {} added, {} skipped",
            imported,
            skipped
        );
        Ok(())
    }

    fn import_keys<H>(
        &mut self,
        keys: Vec<MinisignPublicKey>,
        skip_failed: bool,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        if skip_failed {
            message!(
                message_callback,
                "{}",
                style("Note: --skip-failed has no effect for key imports").yellow()
            );
        }
        let mut config_storage = ConfigStorage::new(&self.paths.config.config_file)?;
        let summary = config_storage.merge_trusted_minisign_keys(&keys)?;
        message!(
            message_callback,
            "Key import complete: {} imported, {} deduped, {} total trusted keys",
            summary.imported,
            summary.deduped,
            summary.total
        );
        Ok(())
    }

    fn parse_minisign_key_file(path: &Path) -> Result<Vec<MinisignPublicKey>> {
        let content = fs::read_to_string(path)
            .context(format!("Failed to read key file '{}'", path.display()))?;
        let mut keys = Vec::new();
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.to_ascii_lowercase().starts_with("untrusted comment:") {
                continue;
            }
            if PublicKey::from_base64(line).is_ok() {
                keys.push(MinisignPublicKey {
                    id: None,
                    key: line.to_string(),
                });
            }
        }

        if keys.is_empty() {
            return Err(anyhow!(
                "No valid minisign public keys found in '{}'",
                path.display()
            ));
        }

        Ok(keys)
    }

    // -----------------------------------------------------------------------
    // Full import (snapshot)
    // -----------------------------------------------------------------------

    fn import_snapshot<H>(&mut self, path: &Path, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        let upstream_dir = &self.paths.dirs.data_dir;
        let upstream_parent = upstream_dir
            .parent()
            .ok_or_else(|| anyhow!("upstream dir has no parent"))?;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();

        // Stage extraction in a temp directory and only swap into place once validated.
        let temp_dir = upstream_parent.join(format!(".upstream-import-{pid}-{unique}"));
        let backup_dir = upstream_parent.join(format!(".upstream-backup-{pid}-{unique}"));
        fs::create_dir_all(&temp_dir).context(format!(
            "Failed to create temporary import directory '{}'",
            temp_dir.display()
        ))?;

        message!(
            message_callback,
            "Extracting snapshot to staging directory ..."
        );

        // Decompress the tarball into temp_dir.  The archive contains an
        // "upstream/" top-level dir, so after extraction we move that into place.
        let extracted =
            crate::services::integration::compression_handler::decompress(path, &temp_dir)
                .context("Failed to extract snapshot")?;

        // The extracted path should be temp_dir/upstream (the top-level dir we
        // created during export).  Rename it directly to .upstream/.
        let source = if extracted.join("upstream").is_dir() {
            extracted.join("upstream")
        } else {
            // Flattened by decompress — the contents are already in extracted.
            extracted.clone()
        };

        if !source.is_dir() {
            return Err(anyhow!(
                "Snapshot extraction did not produce a directory at '{}'",
                source.display()
            ));
        }

        let mut backed_up_existing = false;
        if upstream_dir.exists() {
            message!(
                message_callback,
                "{}",
                style("Existing upstream directory detected; creating rollback backup").yellow()
            );
            fs::rename(upstream_dir, &backup_dir).context(format!(
                "Failed to move existing upstream directory '{}' to backup '{}'",
                upstream_dir.display(),
                backup_dir.display()
            ))?;
            backed_up_existing = true;
        }

        if let Err(err) = fs::rename(&source, upstream_dir) {
            if backed_up_existing {
                let _ = fs::rename(&backup_dir, upstream_dir);
            }
            return Err(err).context(format!(
                "Failed to move extracted snapshot to '{}'",
                upstream_dir.display()
            ));
        }

        if backed_up_existing {
            let _ = fs::remove_dir_all(&backup_dir);
        }

        // Clean up temp dir (may already be gone if source == extracted).
        let _ = fs::remove_dir_all(&temp_dir);

        // Reload storage from the restored files.
        self.package_storage.load_packages().context(
            "Snapshot restored but failed to reload package storage — check the files manually",
        )?;

        message!(
            message_callback,
            "{}",
            style("Snapshot restored successfully").green()
        );

        Ok(())
    }
}

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}
use message;

#[cfg(test)]
mod tests {
    use super::{ImportKind, ImportOperation, is_snapshot};
    use crate::services::storage::package_storage::PackageStorage;
    use crate::utils::static_paths::{
        AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths,
    };
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-import-test-{name}-{nanos}"))
    }

    fn test_paths(root: &Path) -> UpstreamPaths {
        let dirs = AppDirs {
            user_dir: root.to_path_buf(),
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            metadata_dir: root.join("data/metadata"),
        };

        UpstreamPaths {
            config: ConfigPaths {
                config_file: dirs.config_dir.join("config.toml"),
                packages_file: dirs.metadata_dir.join("packages.json"),
                metadata_file: dirs.metadata_dir.join("metadata.json"),
                paths_file: dirs.metadata_dir.join("paths.sh"),
            },
            install: InstallPaths {
                appimages_dir: dirs.data_dir.join("appimages"),
                binaries_dir: dirs.data_dir.join("binaries"),
                archives_dir: dirs.data_dir.join("archives"),
                rollback_dir: dirs.data_dir.join("rollback"),
            },
            integration: IntegrationPaths {
                symlinks_dir: dirs.data_dir.join("symlinks"),
                xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
                icons_dir: dirs.data_dir.join("icons"),
            },
            dirs,
        }
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn snapshot_detection_matches_supported_extensions() {
        assert!(is_snapshot(std::path::Path::new("backup.tar.gz")));
        assert!(is_snapshot(std::path::Path::new("backup.tgz")));
        assert!(!is_snapshot(std::path::Path::new("manifest.json")));
    }

    #[tokio::test]
    async fn import_manifest_rejects_unsupported_manifest_version() {
        let root = temp_root("bad-version");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let manifest_path = root.join("manifest.json");
        fs::write(
                &manifest_path,
                r#"{"version":2,"packages":[{"name":"x","repo_slug":"o/r","filetype":"Binary","channel":"Stable","provider":"Github","base_url":null,"match_pattern":null,"exclude_pattern":null}]}"#,
            )
            .expect("write manifest");

        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let mut operation = ImportOperation::new(&mut storage, &paths);
        let mut dlp: Option<fn(u64, u64)> = None;
        let mut op: Option<fn(u32, u32)> = None;
        let mut msg: Option<fn(&str)> = None;

        let err = operation
            .import(
                &manifest_path,
                false,
                Some(ImportKind::Manifest),
                true,
                &mut dlp,
                &mut op,
                &mut msg,
            )
            .await
            .expect_err("must reject unsupported version");
        assert!(err.to_string().contains("Unsupported manifest version"));

        cleanup(&root).expect("cleanup");
    }
}
