use crate::{
    models::upstream::Package,
    output,
    providers::discovery::friendly_name,
    services::{
        integration::{CompletionManager, DesktopManager, ShellManager, SymlinkManager},
        packaging::{
            PackagePhase, PackageProgressEvent,
            disk_impact::{ByteEstimate, DiskImpact, SignedByteEstimate, estimate_existing_paths},
        },
    },
    storage::database::PackageDatabase,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use dirs;
use std::path::Path;
use std::{fs, io};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct PackageRemover<'a> {
    paths: &'a UpstreamPaths,
}

impl<'a> PackageRemover<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub fn estimate_remove_impact(&self, package: &Package, purge_option: bool) -> DiskImpact {
        let active_size = self.estimate_active_size(package).unwrap_or(0);
        let purge_size = if purge_option {
            estimate_existing_paths(Self::purge_candidate_paths(&package.id)).unwrap_or(0)
        } else {
            0
        };

        DiskImpact {
            download: ByteEstimate::exact(0),
            net: SignedByteEstimate::exact(-i128::from(active_size.saturating_add(purge_size))),
        }
    }

    pub fn estimate_active_size(&self, package: &Package) -> Result<u64> {
        let mut paths = Vec::new();
        if let Some(install_path) = package.install_path.as_ref() {
            paths.push(install_path.clone());
        }

        if let Some(icon_path) = package.icon_path.as_ref() {
            paths.push(icon_path.clone());
        }

        paths.push(self.paths.state.symlinks_dir.join(&package.id));
        paths.push(
            self.paths
                .integration
                .xdg_applications_dir
                .join(format!("{}.desktop", package.id)),
        );
        paths.push(
            self.paths
                .integration
                .bash_completions_dir
                .join(&package.id),
        );
        paths.push(
            self.paths
                .integration
                .fish_completions_dir
                .join(format!("{}.fish", package.id)),
        );
        paths.push(
            self.paths
                .integration
                .zsh_completions_dir
                .join(format!("_{}", package.id)),
        );
        estimate_existing_paths(paths)
    }

    pub fn remove<H, P>(
        &self,
        package_database: &mut PackageDatabase,
        package_name: &str,
        purge_option: bool,
        force_option: bool,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        H: FnMut(&str),
        P: FnMut(&str, PackageProgressEvent),
    {
        let package = package_database
            .get_package(package_name)?
            .ok_or_else(|| anyhow!("Package '{}' is not installed", package_name))?;

        if let Some(callback) = progress_callback.as_mut() {
            callback(
                package_name,
                PackageProgressEvent::Phase(PackagePhase::RemovingPackage),
            );
        }

        if let Err(error) = self
            .remove_package_files(&package, message_callback)
            .context(format!(
                "Failed to perform removal operations for '{}'",
                package_name
            ))
        {
            if !force_option {
                return Err(error);
            }

            if let Some(callback) = message_callback.as_mut() {
                callback(&format!(
                    "{}",
                    output::warning(format!(
                        "Ignoring uninstall error for '{}': {}",
                        package_name, error
                    ))
                ));
            }
        }

        if let Some(callback) = progress_callback.as_mut() {
            callback(
                package_name,
                PackageProgressEvent::Phase(PackagePhase::RemovingMetadata),
            );
        }

        package_database
            .remove_package(&package.id)
            .context(format!(
                "Failed to remove '{}' from package storage",
                package_name
            ))?;
        ShellManager::new(&self.paths.generated.paths_file)
            .regenerate_paths(package_database, self.paths)
            .context(format!(
                "Failed to regenerate PATH integration after removing '{}'",
                package_name
            ))?;

        if !purge_option {
            return Ok(());
        }

        let purge_name = friendly_name(
            &package.provider,
            &package.repo_slug,
            package.base_url.as_deref(),
        )
        .unwrap_or_else(|| package.id.clone());

        if let Some(callback) = progress_callback.as_mut() {
            callback(
                package_name,
                PackageProgressEvent::Phase(PackagePhase::PurgingPackageData),
            );
        }

        if let Err(error) = self
            .purge_configs(&purge_name, message_callback)
            .context(format!(
                "Failed to purge configuration files for '{}'",
                package_name
            ))
        {
            if !force_option {
                return Err(error);
            }

            if let Some(callback) = message_callback.as_mut() {
                callback(&format!(
                    "{}",
                    output::warning(format!(
                        "Ignoring purge error for '{}': {}",
                        package_name, error
                    ))
                ));
            }
        }

        Ok(())
    }

    fn purge_candidate_paths(package_name: &str) -> Vec<std::path::PathBuf> {
        let mut candidates = Vec::new();
        if let Some(config_dir) = dirs::config_dir() {
            candidates.push(config_dir.join(package_name));
            candidates.push(config_dir.join(package_name.to_lowercase()));
        }

        if let Some(cache_dir) = dirs::cache_dir() {
            candidates.push(cache_dir.join(package_name));
            candidates.push(cache_dir.join(package_name.to_lowercase()));
        }

        if let Some(data_dir) = dirs::data_local_dir() {
            candidates.push(data_dir.join(package_name));
            candidates.push(data_dir.join(package_name.to_lowercase()));
        }

        let mut unique = Vec::new();
        for path in candidates {
            if !unique.contains(&path) {
                unique.push(path);
            }
        }

        unique
    }

    /// Remove package files and integrations
    pub fn remove_package_files<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.id))?;

        self.remove_runtime_integrations(package, message_callback)?;

        let metadata = fs::symlink_metadata(install_path).with_context(|| {
            format!(
                "Failed to inspect installation path '{}' (it may have been manually removed)",
                install_path.display()
            )
        })?;

        if metadata.is_dir() {
            message!(
                message_callback,
                "Removing directory: {}",
                install_path.display()
            );

            fs::remove_dir_all(install_path).context(format!(
                "Failed to remove installation directory at '{}'",
                install_path.display()
            ))?;
        } else if metadata.is_file() || metadata.file_type().is_symlink() {
            message!(
                message_callback,
                "Removing file: {}",
                install_path.display()
            );

            fs::remove_file(install_path).context(format!(
                "Failed to remove installation file at '{}'",
                install_path.display()
            ))?;
        } else {
            return Err(anyhow!(
                "Install path '{}' has an unsupported file type",
                install_path.display()
            ));
        }

        if let Some(icon_path) = &package.icon_path {
            message!(message_callback, "Removing desktop entry ...");
            DesktopManager::remove_entry(self.paths, &package.id).context(format!(
                "Failed to remove desktop entry for '{}'",
                package.id
            ))?;

            fs::remove_file(icon_path).context(format!(
                "Failed to remove icon file at '{}'",
                icon_path.display()
            ))?;
            message!(
                message_callback,
                "Removed stored icon: {}",
                icon_path.display()
            );
        }

        Ok(())
    }

    /// Remove PATH and symlink state for a package without deleting installed files.
    pub fn remove_runtime_integrations<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let _ = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.id))?;

        self.remove_runtime_link(package, message_callback)?;

        CompletionManager::new(self.paths)
            .remove_for_package(&package.id, message_callback)
            .context(format!(
                "Failed to remove completion files for '{}'",
                package.id
            ))?;

        Ok(())
    }

    /// Remove only the package's executable link, preserving completions that
    /// are still valid if a replacement install must be rolled back.
    pub fn remove_runtime_link<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let _ = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.id))?;

        for executable in &package.executables {
            message!(
                message_callback,
                "Removing symlink for '{}'",
                executable.name
            );
            SymlinkManager::new(&self.paths.state.symlinks_dir)
                .remove_link(&executable.name)
                .context(format!(
                    "Failed to remove symlink for '{}'",
                    executable.name
                ))?;
        }
        Ok(())
    }

    /// Restore PATH and symlink state for a previously installed package.
    pub fn restore_runtime_integrations<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let _ = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.id))?;

        for executable in &package.executables {
            if executable.path.exists() {
                message!(
                    message_callback,
                    "Restoring symlink for '{}'",
                    executable.name
                );
                SymlinkManager::new(&self.paths.state.symlinks_dir)
                    .add_link(&executable.path, &executable.name)
                    .context(format!(
                        "Failed to restore symlink for '{}'",
                        executable.name
                    ))?;
            }
        }

        Ok(())
    }

    /// Purge configuration files for a package
    pub fn purge_configs<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        message!(
            message_callback,
            "Purging package data for '{}' ...",
            package_name
        );

        // Remove known upstream-owned integration artifacts by package alias.
        DesktopManager::remove_entry(self.paths, package_name).context(format!(
            "Failed to remove desktop entry for '{}'",
            package_name
        ))?;
        CompletionManager::new(self.paths)
            .remove_for_package(package_name, message_callback)
            .context(format!(
                "Failed to remove completion files for '{}'",
                package_name
            ))?;
        self.remove_matching_icons(package_name, message_callback)?;

        // Best-effort XDG/user-dir cleanup for app-owned state.
        for path in Self::purge_candidate_paths(package_name) {
            self.remove_path_if_exists(&path, message_callback)?;
        }

        Ok(())
    }

    fn remove_matching_icons<H>(
        &self,
        package_name: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let icons_dir = &self.paths.state.icons_dir;
        if !icons_dir.exists() {
            return Ok(());
        }

        let package_name_lower = package_name.to_lowercase();
        for entry in fs::read_dir(icons_dir).context(format!(
            "Failed to read icons directory '{}'",
            icons_dir.display()
        ))? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();

            if stem == package_name_lower {
                self.remove_path_if_exists(&path, message_callback)?;
            }
        }

        Ok(())
    }

    fn remove_path_if_exists<H>(&self, path: &Path, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to inspect path '{}'", path.display()));
            }
        };

        if metadata.is_dir() {
            message!(message_callback, "Purging directory: {}", path.display());
            fs::remove_dir_all(path)
                .context(format!("Failed to remove directory '{}'", path.display()))?;
        } else if metadata.is_file() || metadata.file_type().is_symlink() {
            message!(message_callback, "Purging file: {}", path.display());
            fs::remove_file(path).context(format!("Failed to remove file '{}'", path.display()))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::PackageRemover;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::utils::test_support;
    use std::path::Path;
    use std::{fs, io};

    #[cfg(test)]
    fn managed_path_entry(
        package_remover: &PackageRemover,
        package: &Package,
        install_path: &Path,
    ) -> Option<std::path::PathBuf> {
        if package.filetype != crate::models::common::enums::Filetype::Archive
            || !install_path.starts_with(&package_remover.paths.install.archives_dir)
        {
            return None;
        }

        if install_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
        {
            return None;
        }

        package
            .primary_executable()
            .and_then(|executable| executable.path.parent().map(Path::to_path_buf))
            .or_else(|| Some(install_path.to_path_buf()))
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        test_support::temp_root("upstream-remover-test", name)
    }

    fn test_paths(root: &Path) -> crate::utils::static_paths::UpstreamPaths {
        test_support::upstream_paths(root)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn remove_path_if_exists_deletes_file_and_directory() {
        let root = temp_root("remove-path");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.state.icons_dir).expect("create icons dir");
        let file = paths.state.icons_dir.join("pkg.png");
        fs::write(&file, b"icon").expect("write icon");
        let nested_dir = root.join("to-remove");
        fs::create_dir_all(nested_dir.join("x")).expect("create nested dir");

        let remover = PackageRemover::new(&paths);
        let mut messages = Some(|_: &str| {});
        remover
            .remove_path_if_exists(&file, &mut messages)
            .expect("remove file");
        remover
            .remove_path_if_exists(&nested_dir, &mut messages)
            .expect("remove directory");
        remover
            .remove_path_if_exists(&root.join("missing"), &mut messages)
            .expect("ignore missing");

        assert!(!file.exists());
        assert!(!nested_dir.exists());

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn managed_path_entry_uses_archive_executable_parent() {
        let root = temp_root("path-entry-exec-parent");
        let paths = test_paths(&root);
        let install_path = paths.install.archives_dir.join("tool");
        let exec_parent = install_path.join("bin");
        let exec_path = exec_parent.join("tool");

        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );

        package.install_path = Some(install_path.clone());
        package.executables = vec![crate::models::upstream::PackageExecutable {
            path: exec_path,
            name: "tool".to_string(),
        }];
        fs::create_dir_all(&exec_parent).expect("create exec parent");

        let remover = PackageRemover::new(&paths);

        assert_eq!(
            managed_path_entry(&remover, &package, &install_path),
            Some(exec_parent)
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn managed_path_entry_skips_non_archive_installs() {
        let root = temp_root("path-entry-non-archive");
        let paths = test_paths(&root);
        let install_path = paths.install.binaries_dir.join("tool");

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
        package.executables = vec![crate::models::upstream::PackageExecutable {
            path: install_path.clone(),
            name: "tool".to_string(),
        }];
        fs::create_dir_all(install_path.parent().expect("parent")).expect("create parent");
        fs::write(&install_path, b"bin").expect("write binary");

        let remover = PackageRemover::new(&paths);

        assert_eq!(managed_path_entry(&remover, &package, &install_path), None);

        cleanup(&root).expect("cleanup");
    }
}
