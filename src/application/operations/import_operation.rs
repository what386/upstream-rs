use crate::{
    application::operations::install_operation::InstallOperation,
    models::common::enums::TrustMode,
    models::upstream::PackageReference,
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{PackageInstaller, PackageRemover, PackageUpgrader},
        storage::package_storage::PackageStorage,
        trust::MinisignPublicKey,
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use console::style;
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

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Returns true if the path looks like a tarball we produced.
fn is_snapshot(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name.ends_with(".tar.gz") || name.ends_with(".tgz")
}

// ---------------------------------------------------------------------------
// Operation
// ---------------------------------------------------------------------------

pub struct ImportOperation<'a> {
    provider_manager: &'a ProviderManager,
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
    trusted_keys: Vec<MinisignPublicKey>,
}

impl<'a> ImportOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
        trusted_keys: Vec<MinisignPublicKey>,
    ) -> Self {
        Self {
            provider_manager,
            package_storage,
            paths,
            trusted_keys,
        }
    }

    /// Entry point — dispatches to manifest or snapshot based on the file.
    pub async fn import<F, G, H>(
        &mut self,
        path: &Path,
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
        if is_snapshot(path) {
            if skip_failed {
                message!(
                    message_callback,
                    "{}",
                    style("Note: --skip-failed has no effect for snapshot imports").yellow()
                );
            }
            self.import_snapshot(path, message_callback)
        } else {
            self.import_manifest(
                path,
                skip_failed,
                download_progress_callback,
                overall_progress_callback,
                message_callback,
            )
            .await
        }
    }

    // -----------------------------------------------------------------------
    // Light import (manifest)
    // -----------------------------------------------------------------------

    async fn import_manifest<F, G, H>(
        &mut self,
        path: &Path,
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
        let content = fs::read_to_string(path)
            .context(format!("Failed to read manifest from '{}'", path.display()))?;

        let manifest: ImportManifest =
            serde_json::from_str(&content).context("Failed to parse manifest")?;

        if manifest.version != 1 {
            return Err(anyhow!(
                "Unsupported manifest version {}. Upgrade upstream and try again.",
                manifest.version
            ));
        }

        // Split into packages that need installing vs upgrading (force).
        let installed_names: std::collections::HashSet<&str> = self
            .package_storage
            .get_all_packages()
            .iter()
            .filter(|p| p.install_path.is_some())
            .map(|p| p.name.as_str())
            .collect();

        let mut to_install = Vec::new();
        let mut to_upgrade = Vec::new();
        let mut failures = 0_u32;

        for reference in &manifest.packages {
            if installed_names.contains(reference.name.as_str()) {
                to_upgrade.push(reference.clone());
            } else {
                to_install.push(reference.clone());
            }
        }

        // --- Upgrade already-installed packages (force) ---
        if !to_upgrade.is_empty() {
            message!(
                message_callback,
                "{}",
                style(format!(
                    "{} package(s) already installed — forcing upgrade",
                    to_upgrade.len()
                ))
                .yellow()
            );

            let installer = PackageInstaller::new(self.provider_manager, self.paths)?;
            let remover = PackageRemover::new(self.paths);
            let upgrader = PackageUpgrader::new(
                self.provider_manager,
                installer,
                remover,
                self.paths,
                self.trusted_keys.clone(),
            );

            for reference in &to_upgrade {
                let Some(package) = self
                    .package_storage
                    .get_package_by_name(&reference.name)
                    .cloned()
                else {
                    if skip_failed {
                        failures += 1;
                        message!(
                            message_callback,
                            "{} Package '{}' missing from storage; skipping",
                            style("Upgrade failed:").red(),
                            reference.name
                        );
                        continue;
                    }
                    return Err(anyhow!("Package '{}' not found in storage", reference.name));
                };

                message!(message_callback, "Upgrading '{}' ...", reference.name);

                match upgrader
                    .upgrade(
                        &package,
                        true,
                        TrustMode::BestEffort,
                        download_progress_callback,
                        message_callback,
                    )
                    .await
                {
                    Ok(Some(updated)) => {
                        self.package_storage.add_or_update_package(updated)?;
                        message!(
                            message_callback,
                            "{}",
                            style(format!("'{}' upgraded", reference.name)).green()
                        );
                    }
                    Ok(None) => {
                        // Shouldn't happen with force=true, but harmless
                        message!(message_callback, "'{}' already up to date", reference.name);
                    }
                    Err(e) => {
                        if skip_failed {
                            failures += 1;
                            message!(message_callback, "{} {}", style("Upgrade failed:").red(), e);
                        } else {
                            return Err(e).context(format!(
                                "Failed to upgrade package '{}'",
                                reference.name
                            ));
                        }
                    }
                }
            }
        }

        // --- Install new packages via existing bulk path ---
        if !to_install.is_empty() {
            message!(
                message_callback,
                "Installing {} new package(s) ...",
                to_install.len()
            );

            let packages: Vec<_> = to_install.into_iter().map(|r| r.into_package()).collect();

            let mut install_op = InstallOperation::new(
                self.provider_manager,
                self.package_storage,
                self.paths,
                self.trusted_keys.clone(),
            )?;
            let total = packages.len() as u32;
            let mut completed = 0_u32;

            for package in packages {
                let package_name = package.name.clone();
                let use_icon = package.icon_path.is_some();
                message!(message_callback, "Installing '{}' ...", package_name);

                let install_result = install_op
                    .install_single(
                        package,
                        &None,
                        &use_icon,
                        TrustMode::BestEffort,
                        download_progress_callback,
                        message_callback,
                    )
                    .await
                    .context(format!("Failed to install package '{}'", package_name));

                match install_result {
                    Ok(_) => {
                        message!(
                            message_callback,
                            "{}",
                            style(format!("'{}' installed", package_name)).green()
                        );
                    }
                    Err(err) => {
                        if skip_failed {
                            failures += 1;
                            message!(
                                message_callback,
                                "{} {}",
                                style("Install failed:").red(),
                                err
                            );
                        } else {
                            return Err(err);
                        }
                    }
                }

                completed += 1;
                if let Some(cb) = overall_progress_callback.as_mut() {
                    cb(completed, total);
                }
            }
        }

        if skip_failed && failures > 0 {
            message!(
                message_callback,
                "{} package(s) failed during import but were skipped",
                failures
            );
        }

        Ok(())
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
    use super::{ImportOperation, is_snapshot};
    use crate::providers::provider_manager::ProviderManager;
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
                paths_file: dirs.metadata_dir.join("paths.sh"),
            },
            install: InstallPaths {
                appimages_dir: dirs.data_dir.join("appimages"),
                binaries_dir: dirs.data_dir.join("binaries"),
                archives_dir: dirs.data_dir.join("archives"),
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
        let manager = ProviderManager::new(None, None, None).expect("provider manager");
        let mut operation = ImportOperation::new(&manager, &mut storage, &paths, Vec::new());
        let mut dlp: Option<fn(u64, u64)> = None;
        let mut op: Option<fn(u32, u32)> = None;
        let mut msg: Option<fn(&str)> = None;

        let err = operation
            .import(&manifest_path, false, &mut dlp, &mut op, &mut msg)
            .await
            .expect_err("must reject unsupported version");
        assert!(err.to_string().contains("Unsupported manifest version"));

        cleanup(&root).expect("cleanup");
    }
}
