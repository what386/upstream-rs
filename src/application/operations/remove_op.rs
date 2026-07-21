use crate::{
    application::cancellation,
    output::{self, Status},
    services::packaging::{PackagePhase, PackageProgressEvent, PackageRemover},
    services::{integration::ShellManager, packaging::disk_impact::DiskImpact},
    storage::database::PackageDatabase,
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
    package_database: &'a mut PackageDatabase,
}

pub struct RemovePreview {
    pub impact: DiskImpact,
    pub items: Vec<RemovePreviewItem>,
}

pub struct RemovePreviewItem {
    pub name: String,
    pub status: RemovePreviewStatus,
}

pub enum RemovePreviewStatus {
    Planned,
    Failed { error: String },
}

impl<'a> RemoveOperation<'a> {
    pub fn new(package_database: &'a mut PackageDatabase, paths: &'a UpstreamPaths) -> Self {
        let remover = PackageRemover::new(paths);
        Self {
            remover,
            package_database,
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

        if let Some(cb) = overall_progress_callback.as_mut() {
            cb(0, total);
        }

        for package_name in package_names {
            cancellation::check()?;
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

    pub fn preview_bulk(&self, package_names: &[String], purge_option: bool) -> RemovePreview {
        let mut impact = DiskImpact::empty();
        let mut items = Vec::with_capacity(package_names.len());

        for package_name in package_names {
            match self.package_database.get_package(package_name) {
                Ok(Some(package)) => {
                    impact = impact + self.remover.estimate_remove_impact(&package, purge_option);
                    items.push(RemovePreviewItem {
                        name: package_name.clone(),
                        status: RemovePreviewStatus::Planned,
                    });
                }
                Ok(None) => items.push(RemovePreviewItem {
                    name: package_name.clone(),
                    status: RemovePreviewStatus::Failed {
                        error: format!("Package '{}' is not installed", package_name),
                    },
                }),
                Err(err) => items.push(RemovePreviewItem {
                    name: package_name.clone(),
                    status: RemovePreviewStatus::Failed {
                        error: output::error_summary(&err),
                    },
                }),
            }
        }

        RemovePreview { impact, items }
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
                    .package_database
                    .get_package(package_name)?
                    .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;
                Ok((
                    format!("{}/{}", package.provider, package.name),
                    package.version.to_string(),
                    self.remover.estimate_remove_impact(&package, purge_option),
                ))
            })
            .collect()
    }

    pub fn remove_single<H, P>(
        &mut self,
        package_name: &str,
        purge_option: &bool,
        force_option: &bool,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        H: FnMut(&str),
        P: FnMut(&str, PackageProgressEvent),
    {
        let package = self
            .package_database
            .get_package(package_name)?
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

        progress!(
            progress_callback,
            package_name,
            PackageProgressEvent::Phase(PackagePhase::RemovingPackage)
        );
        let removal_result = self
            .remover
            .remove_package_files(&package, message_callback)
            .context(format!(
                "Failed to perform removal operations for '{}'",
                package_name
            ));

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
        self.package_database
            .remove_package(package_name)
            .context(format!(
                "Failed to remove '{}' from package storage",
                package_name
            ))?;

        let paths = self.remover.paths();
        ShellManager::new(&paths.config.paths_file)
            .regenerate_paths(self.package_database, paths)
            .context(format!(
                "Failed to regenerate PATH integration after removing '{}'",
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
    use super::{RemoveOperation, RemovePreviewStatus};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::services::packaging::PackageProgressEvent;
    use crate::storage::database::PackageDatabase;
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
        let mut storage =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
        let mut op = RemoveOperation::new(&mut storage, &paths);
        let mut msg = Some(|_: &str| {});
        let mut remove_progress: Option<fn(&str, PackageProgressEvent)> = None;

        let err = op
            .remove_single("missing", &false, &false, &mut msg, &mut remove_progress)
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
        let mut storage =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
        let mut op = RemoveOperation::new(&mut storage, &paths);
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
        assert_eq!(progress_calls.first().copied(), Some((0, 2)));
        assert_eq!(progress_calls.last().copied(), Some((2, 2)));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn remove_single_does_not_capture_rollback_artifacts() {
        let root = temp_root("no-rollback");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");

        let install_path = paths.install.binaries_dir.join("tool");
        fs::write(&install_path, b"binary").expect("write install");

        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(install_path.clone());
        package.exec_path = Some(install_path.clone());

        let mut storage =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
        storage.upsert_package(&package).expect("store package");

        let mut op = RemoveOperation::new(&mut storage, &paths);
        let mut msg = Some(|_: &str| {});
        let mut progress: Option<fn(&str, PackageProgressEvent)> = None;

        op.remove_single(&package.name, &false, &false, &mut msg, &mut progress)
            .expect("remove package");

        assert!(!install_path.exists());
        assert!(!paths.state.rollback_dir.join("tool").exists());
        assert!(!paths.dirs.metadata_dir.join("rollback.json").exists());

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn preview_bulk_preserves_order_and_reports_missing_packages() {
        let root = temp_root("preview-order");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
        let package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        storage.upsert_package(&package).expect("store package");
        let op = RemoveOperation::new(&mut storage, &paths);

        let preview = op.preview_bulk(&["tool".to_string(), "missing".to_string()], false);
        assert_eq!(preview.items[0].name, "tool");
        assert!(matches!(
            preview.items[0].status,
            RemovePreviewStatus::Planned
        ));
        assert_eq!(preview.items[1].name, "missing");
        assert!(matches!(
            &preview.items[1].status,
            RemovePreviewStatus::Failed { error } if error.contains("is not installed")
        ));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn preview_bulk_reports_missing_without_mutating_storage() {
        let root = temp_root("preview-bulk");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
        let op = RemoveOperation::new(&mut storage, &paths);

        let names = vec!["a".to_string(), "b".to_string()];
        let preview = op.preview_bulk(&names, false);
        assert_eq!(preview.items.len(), 2);
        assert!(
            preview
                .items
                .iter()
                .all(|item| matches!(&item.status, RemovePreviewStatus::Failed { .. }))
        );

        let persisted =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage reload");
        assert!(persisted.list_packages().expect("list packages").is_empty());

        cleanup(&root).expect("cleanup");
    }
}
