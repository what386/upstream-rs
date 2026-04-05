use crate::services::packaging::PackageRemover;
use crate::{
    services::storage::package_storage::PackageStorage, utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use console::style;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct RemoveOperation<'a> {
    remover: PackageRemover<'a>,
    package_storage: &'a mut PackageStorage,
}

impl<'a> RemoveOperation<'a> {
    pub fn new(package_storage: &'a mut PackageStorage, paths: &'a UpstreamPaths) -> Self {
        let remover = PackageRemover::new(paths);
        Self {
            remover,
            package_storage,
        }
    }

    pub fn remove_bulk<H, G>(
        &mut self,
        package_names: &Vec<String>,
        purge_option: &bool,
        message_callback: &mut Option<H>,
        overall_progress_callback: &mut Option<G>,
    ) -> Result<(u32, u32)>
    where
        H: FnMut(&str),
        G: FnMut(u32, u32),
    {
        let total = package_names.len() as u32;
        let mut completed = 0;
        let mut failures = 0;

        for package_name in package_names {
            message!(message_callback, "Removing '{}' ...", package_name);

            match self
                .remove_single(package_name, purge_option, message_callback)
                .context(format!("Failed to remove package '{}'", package_name))
            {
                Ok(_) => message!(message_callback, "{}", style("Package removed").green()),
                Err(e) => {
                    message!(message_callback, "{} {}", style("Removal failed:").red(), e);
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

    pub fn remove_single<H>(
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
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

        self.remover
            .remove_package_files(package, message_callback)
            .context(format!(
                "Failed to perform removal operations for '{}'",
                package_name
            ))?;

        self.package_storage
            .remove_package_by_name(package_name)
            .context(format!(
                "Failed to remove '{}' from package storage",
                package_name
            ))?;

        if *purge_option {
            self.remover
                .purge_configs(package_name, message_callback)
                .context(format!(
                    "Failed to purge configuration files for '{}'",
                    package_name
                ))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::RemoveOperation;
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
        std::env::temp_dir().join(format!("upstream-remove-op-test-{name}-{nanos}"))
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
    fn remove_single_returns_error_for_missing_package() {
        let root = temp_root("missing");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let mut op = RemoveOperation::new(&mut storage, &paths);
        let mut msg: Option<fn(&str)> = None;

        let err = op
            .remove_single("missing", &false, &mut msg)
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
        let mut op = RemoveOperation::new(&mut storage, &paths);
        let mut msg: Option<fn(&str)> = None;
        let mut progress_calls = Vec::new();
        let mut progress = Some(|done: u32, total: u32| {
            progress_calls.push((done, total));
        });
        let names = vec!["a".to_string(), "b".to_string()];

        let (removed, failed) = op
            .remove_bulk(&names, &false, &mut msg, &mut progress)
            .expect("bulk remove");
        assert_eq!((removed, failed), (0, 2));
        assert_eq!(progress_calls.last().copied(), Some((2, 2)));

        cleanup(&root).expect("cleanup");
    }
}
