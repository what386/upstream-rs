use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;

use crate::models::upstream::Package;
use crate::services::packaging::PackageRemover;
use crate::services::storage::{
    metadata_storage::MetadataStorage,
    package_storage::PackageStorage,
    rollback_storage::{RollbackRecord, RollbackSource, RollbackStorage},
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

impl<'a> RollbackManager<'a> {
    pub fn rollback_file_path(paths: &UpstreamPaths) -> PathBuf {
        paths.dirs.metadata_dir.join("rollback.json")
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

        let package_rollback_dir = self.paths.install.rollback_dir.join(&package.name);
        if package_rollback_dir.exists() {
            fs::remove_dir_all(&package_rollback_dir).context(format!(
                "Failed to clear existing rollback directory '{}'",
                package_rollback_dir.display()
            ))?;
        }
        fs::create_dir_all(&package_rollback_dir).context(format!(
            "Failed to create rollback directory '{}'",
            package_rollback_dir.display()
        ))?;

        let install_name = install_path.file_name().ok_or_else(|| {
            anyhow!(
                "Install path '{}' has no final file name",
                install_path.display()
            )
        })?;
        let rollback_artifact = package_rollback_dir.join(install_name);
        message!(
            message_callback,
            "Capturing rollback artifact for '{}' at '{}'",
            package.name,
            rollback_artifact.display()
        );
        safe_move::move_file_or_dir(install_path, &rollback_artifact)?;

        let icon_relative_path = if let Some(icon_path) = package.icon_path.as_ref() {
            if icon_path.exists() {
                let icon_name = icon_path.file_name().ok_or_else(|| {
                    anyhow!("Icon path '{}' has no file name", icon_path.display())
                })?;
                let icon_backup =
                    package_rollback_dir.join(format!("icon-{}", icon_name.to_string_lossy()));
                fs::copy(icon_path, &icon_backup).context(format!(
                    "Failed to copy icon '{}' to '{}'",
                    icon_path.display(),
                    icon_backup.display()
                ))?;
                Some(path_relative_to(
                    &self.paths.install.rollback_dir,
                    &icon_backup,
                )?)
            } else {
                None
            }
        } else {
            None
        };

        let record = RollbackRecord {
            package_snapshot: package.clone(),
            artifact_relative_path: path_relative_to(
                &self.paths.install.rollback_dir,
                &rollback_artifact,
            )?,
            icon_relative_path,
            source,
            created_at: Utc::now(),
        };
        self.rollback_storage.upsert_record(&package.name, record)?;
        Ok(())
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

        let source_path = self
            .paths
            .install
            .rollback_dir
            .join(&record.artifact_relative_path);
        if !source_path.exists() {
            return Err(anyhow!(
                "Rollback artifact is missing for '{}': {}",
                package_name,
                source_path.display()
            ));
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
        safe_move::move_file_or_dir(&source_path, &target_install_path)?;

        if let (Some(icon_rel), Some(icon_target)) = (
            record.icon_relative_path.as_ref(),
            record.package_snapshot.icon_path.as_ref(),
        ) {
            let icon_source = self.paths.install.rollback_dir.join(icon_rel);
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
        let package_dir = self.paths.install.rollback_dir.join(package_name);
        if package_dir.exists() {
            let _ = fs::remove_dir_all(package_dir);
        }

        Ok(())
    }

    pub fn prune_package(&mut self, package_name: &str) -> Result<bool> {
        let removed = self.rollback_storage.remove_record(package_name)?.is_some();
        if removed {
            let package_dir = self.paths.install.rollback_dir.join(package_name);
            if package_dir.exists() {
                fs::remove_dir_all(&package_dir).context(format!(
                    "Failed to remove rollback directory '{}'",
                    package_dir.display()
                ))?;
            }
        }
        Ok(removed)
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
