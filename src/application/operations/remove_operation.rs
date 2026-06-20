use crate::{
    output::{self, Status},
    services::packaging::disk_impact::{DiskImpact, SignedByteEstimate, estimate_path_size},
    services::packaging::{PackagePhase, PackageProgressEvent, PackageRemover, RollbackManager},
    services::storage::rollback_storage::RollbackSource,
    services::storage::{metadata_storage::MetadataStorage, package_storage::PackageStorage},
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

macro_rules! progress {
    ($cb:expr, $name:expr, $event:expr) => {{
        if let Some(cb) = $cb.as_mut() {
            cb($name, $event);
        }
    }};
}

pub struct RemoveOperation<'a> {
    remover: PackageRemover<'a>,
    package_storage: &'a mut PackageStorage,
    metadata_storage: &'a mut MetadataStorage,
    paths: &'a UpstreamPaths,
}

impl<'a> RemoveOperation<'a> {
    pub fn new(
        package_storage: &'a mut PackageStorage,
        metadata_storage: &'a mut MetadataStorage,
        paths: &'a UpstreamPaths,
    ) -> Self {
        let remover = PackageRemover::new(paths);
        Self {
            remover,
            package_storage,
            metadata_storage,
            paths,
        }
    }

    pub fn remove_bulk<H, G, P>(
        &mut self,
        package_names: &Vec<String>,
        purge_option: &bool,
        force_option: &bool,
        message_callback: &mut Option<H>,
        overall_progress_callback: &mut Option<G>,
        progress_callback: &mut Option<P>,
    ) -> Result<(u32, u32)>
    where
        H: FnMut(&str),
        G: FnMut(u32, u32),
        P: FnMut(&str, PackageProgressEvent),
    {
        let total = package_names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;
        let completion_subject_width =
            output::status_subject_width(package_names.iter().map(String::as_str));

        for package_name in package_names {
            progress!(
                progress_callback,
                package_name,
                PackageProgressEvent::Phase(PackagePhase::RemovingPackage)
            );

            match self
                .remove_single(
                    package_name,
                    purge_option,
                    force_option,
                    RollbackSource::Remove,
                    message_callback,
                    progress_callback,
                )
                .context(format!("Failed to remove package '{}'", package_name))
            {
                Ok(_) => message!(
                    message_callback,
                    "{}",
                    output::status_line_text_with_width(
                        Status::Ok,
                        package_name,
                        "removed",
                        completion_subject_width
                    )
                ),
                Err(e) => {
                    message!(
                        message_callback,
                        "{}",
                        output::status_line_text_with_width(
                            Status::Fail,
                            package_name,
                            output::error_summary(&e),
                            completion_subject_width
                        )
                    );
                    failures += 1;
                }
            }

            completed += 1;
            if let Some(cb) = overall_progress_callback.as_mut() {
                cb(completed, total);
            }
        }

        if failures > 0 {
            message!(
                message_callback,
                "{} package(s) failed to be removed",
                failures
            );
        }

        let removed = total - failures;
        Ok((removed, failures))
    }

    pub fn preview_bulk<H>(
        &mut self,
        package_names: &Vec<String>,
        purge_option: &bool,
        message_callback: &mut Option<H>,
    ) -> Result<(u32, u32)>
    where
        H: FnMut(&str),
    {
        let mut planned = 0;
        let mut failures = 0;
        let completion_subject_width =
            output::status_subject_width(package_names.iter().map(String::as_str));

        for package_name in package_names {
            match self.preview_single(package_name, purge_option, message_callback) {
                Ok(_) => planned += 1,
                Err(err) => {
                    message!(
                        message_callback,
                        "{}",
                        output::status_line_text_with_width(
                            Status::Fail,
                            package_name,
                            output::error_summary(&err),
                            completion_subject_width
                        )
                    );
                    failures += 1;
                }
            }
        }

        Ok((planned, failures))
    }

    pub fn estimate_bulk_impact(
        &self,
        package_names: &[String],
        purge_option: bool,
    ) -> (DiskImpact, u32, u32) {
        let mut impact = DiskImpact::empty();
        let mut planned = 0_u32;
        let mut failures = 0_u32;

        for package_name in package_names {
            let Some(package) = self.package_storage.get_package_by_name(package_name) else {
                failures += 1;
                continue;
            };
            impact = impact + self.remover.estimate_remove_impact(package, purge_option);
            planned += 1;
        }

        (impact, planned, failures)
    }

    pub fn transaction_impact_rows(
        &self,
        package_names: &[String],
        purge_option: bool,
    ) -> Result<Vec<(String, String, DiskImpact)>> {
        package_names
            .iter()
            .map(|package_name| {
                let package = self
                    .package_storage
                    .get_package_by_name(package_name)
                    .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;
                Ok((
                    format!("{}/{}", package.provider, package.name),
                    package.version.to_string(),
                    self.remover.estimate_remove_impact(package, purge_option),
                ))
            })
            .collect()
    }

    pub fn estimate_rollback_impact(
        &self,
        package_names: &[String],
        purge_option: bool,
    ) -> SignedByteEstimate {
        if purge_option {
            return SignedByteEstimate::exact(0);
        }

        package_names
            .iter()
            .map(|package_name| {
                let Some(package) = self.package_storage.get_package_by_name(package_name) else {
                    return SignedByteEstimate::unknown();
                };
                let active_size = self.remover.estimate_active_size(package).unwrap_or(0);
                let existing_rollback =
                    estimate_path_size(&self.paths.install.rollback_dir.join(&package.name))
                        .unwrap_or(0);
                SignedByteEstimate::exact(
                    i128::from(active_size).saturating_sub(i128::from(existing_rollback)),
                )
            })
            .fold(SignedByteEstimate::exact(0), |total, impact| total + impact)
    }

    pub fn preview_single<H>(
        &mut self,
        package_name: &str,
        purge_option: &bool,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let package = self
            .package_storage
            .get_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?
            .clone();

        let install_path = package
            .install_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<missing>".to_string());
        let exec_path = package
            .exec_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());

        message!(
            message_callback,
            "{:<7} {:<28} would remove runtime files at {}",
            "[plan]",
            package.name,
            install_path
        );
        message!(
            message_callback,
            "        {:<28} would remove symlink/metadata (exec: {})",
            package.name,
            exec_path
        );
        if *purge_option {
            message!(
                message_callback,
                "        {:<28} would purge app-owned config/cache/data",
                package.name
            );
        }

        Ok(())
    }

    pub fn remove_single<H, P>(
        &mut self,
        package_name: &str,
        purge_option: &bool,
        force_option: &bool,
        rollback_source: RollbackSource,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        H: FnMut(&str),
        P: FnMut(&str, PackageProgressEvent),
    {
        let package = self
            .package_storage
            .get_package_by_name(package_name)
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?
            .clone();

        let mut rollback_captured = false;
        if !*purge_option {
            progress!(
                progress_callback,
                package_name,
                PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot)
            );
            let rollback_file = RollbackManager::rollback_file_path(self.paths);
            let mut rollback_storage =
                crate::services::storage::rollback_storage::RollbackStorage::new(&rollback_file)?;
            let mut rollback_manager = RollbackManager::new(
                self.paths,
                self.package_storage,
                self.metadata_storage,
                &mut rollback_storage,
            );
            if let Err(err) =
                rollback_manager.capture_from_installed(&package, rollback_source, message_callback)
            {
                progress!(
                    progress_callback,
                    package_name,
                    PackageProgressEvent::Warning(format!(
                        "Warning: failed to capture rollback: {err}"
                    ))
                );
            } else {
                rollback_captured = true;
            }
        }

        let removal_result = if rollback_captured {
            progress!(
                progress_callback,
                package_name,
                PackageProgressEvent::Phase(PackagePhase::RemovingRuntimeLinks)
            );
            self.remover
                .remove_runtime_and_desktop_artifacts(&package, message_callback)
                .context(format!(
                    "Failed to perform removal operations for '{}'",
                    package_name
                ))
        } else {
            progress!(
                progress_callback,
                package_name,
                PackageProgressEvent::Phase(PackagePhase::RemovingPackage)
            );
            self.remover
                .remove_package_files(&package, message_callback)
                .context(format!(
                    "Failed to perform removal operations for '{}'",
                    package_name
                ))
        };

        if let Err(err) = removal_result {
            if !*force_option {
                return Err(err);
            }
            message!(
                message_callback,
                "{}",
                output::warning(format!(
                    "Ignoring uninstall error for '{}': {}",
                    package_name, err
                ))
            );
        }

        progress!(
            progress_callback,
            package_name,
            PackageProgressEvent::Phase(PackagePhase::RemovingMetadata)
        );
        self.package_storage
            .remove_package_by_name(package_name)
            .context(format!(
                "Failed to remove '{}' from package storage",
                package_name
            ))?;
        self.metadata_storage
            .remove_package(package_name)
            .context(format!(
                "Failed to remove '{}' from sidecar metadata",
                package_name
            ))?;

        if *purge_option {
            progress!(
                progress_callback,
                package_name,
                PackageProgressEvent::Phase(PackagePhase::PurgingPackageData)
            );
            let purge_result = self
                .remover
                .purge_configs(package_name, message_callback)
                .context(format!(
                    "Failed to purge configuration files for '{}'",
                    package_name
                ));
            if let Err(err) = purge_result {
                if !*force_option {
                    return Err(err);
                }
                message!(
                    message_callback,
                    "{}",
                    output::warning(format!(
                        "Ignoring purge error for '{}': {}",
                        package_name, err
                    ))
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::RemoveOperation;
    use crate::services::packaging::PackageProgressEvent;
    use crate::services::storage::rollback_storage::RollbackSource;
    use crate::services::storage::{
        metadata_storage::MetadataStorage, package_storage::PackageStorage,
    };
    use crate::utils::test_support;
    use std::path::Path;
    use std::{fs, io};

    fn temp_root(name: &str) -> std::path::PathBuf {
        test_support::temp_root("upstream-remove-op-test", name)
    }

    fn test_paths(root: &Path) -> crate::utils::static_paths::UpstreamPaths {
        test_support::upstream_paths(root)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn remove_single_returns_error_for_missing_package() {
        let root = temp_root("missing");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let mut metadata_storage =
            MetadataStorage::new(&paths.config.metadata_file).expect("metadata");
        let mut op = RemoveOperation::new(&mut storage, &mut metadata_storage, &paths);
        let mut msg = Some(|_: &str| {});
        let mut remove_progress: Option<fn(&str, PackageProgressEvent)> = None;

        let err = op
            .remove_single(
                "missing",
                &false,
                &false,
                RollbackSource::Remove,
                &mut msg,
                &mut remove_progress,
            )
            .expect_err("missing package");
        assert!(err.to_string().contains("is not installed"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn remove_bulk_reports_failures_for_missing_packages() {
        let root = temp_root("bulk");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let mut metadata_storage =
            MetadataStorage::new(&paths.config.metadata_file).expect("metadata");
        let mut op = RemoveOperation::new(&mut storage, &mut metadata_storage, &paths);
        let mut msg = Some(|_: &str| {});
        let mut progress_calls = Vec::new();
        let mut progress = Some(|done: u32, total: u32| {
            progress_calls.push((done, total));
        });
        let mut remove_progress: Option<fn(&str, PackageProgressEvent)> = None;
        let names = vec!["a".to_string(), "b".to_string()];

        let (removed, failed) = op
            .remove_bulk(
                &names,
                &false,
                &false,
                &mut msg,
                &mut progress,
                &mut remove_progress,
            )
            .expect("bulk remove");
        assert_eq!((removed, failed), (0, 2));
        assert_eq!(progress_calls.last().copied(), Some((2, 2)));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn preview_single_returns_error_for_missing_package() {
        let root = temp_root("preview-missing");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let mut metadata_storage =
            MetadataStorage::new(&paths.config.metadata_file).expect("metadata");
        let mut op = RemoveOperation::new(&mut storage, &mut metadata_storage, &paths);
        let mut msg = Some(|_: &str| {});

        let err = op
            .preview_single("missing", &false, &mut msg)
            .expect_err("missing package");
        assert!(err.to_string().contains("is not installed"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn preview_bulk_reports_missing_without_mutating_storage() {
        let root = temp_root("preview-bulk");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let mut metadata_storage =
            MetadataStorage::new(&paths.config.metadata_file).expect("metadata");
        let mut op = RemoveOperation::new(&mut storage, &mut metadata_storage, &paths);
        let mut msg = Some(|_: &str| {});

        let names = vec!["a".to_string(), "b".to_string()];
        let (planned, failed) = op
            .preview_bulk(&names, &false, &mut msg)
            .expect("preview bulk");
        assert_eq!((planned, failed), (0, 2));

        let persisted = PackageStorage::new(&paths.config.packages_file).expect("storage reload");
        assert!(persisted.get_all_packages().is_empty());

        cleanup(&root).expect("cleanup");
    }
}
