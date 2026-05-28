use crate::{
    models::common::enums::Filetype,
    models::upstream::Package,
    services::integration::{CompletionManager, DesktopManager, ShellManager, SymlinkManager},
    services::packaging::disk_impact::{
        ByteEstimate, DiskImpact, SignedByteEstimate, estimate_existing_paths,
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use dirs;
use std::fs;
use std::path::Path;

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
    fn managed_path_entry(
        &self,
        package: &Package,
        install_path: &Path,
    ) -> Option<std::path::PathBuf> {
        if package.filetype != Filetype::Archive
            || !install_path.starts_with(&self.paths.install.archives_dir)
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
            .exec_path
            .as_ref()
            .and_then(|exec_path| exec_path.parent().map(Path::to_path_buf))
            .or_else(|| Some(install_path.to_path_buf()))
    }

    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub fn estimate_remove_impact(&self, package: &Package, purge_option: bool) -> DiskImpact {
        let active_size = self.estimate_active_size(package).unwrap_or(0);
        let purge_size = if purge_option {
            estimate_existing_paths(Self::purge_candidate_paths(&package.name)).unwrap_or(0)
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
        paths.push(self.paths.integration.symlinks_dir.join(&package.name));
        paths.push(
            self.paths
                .integration
                .xdg_applications_dir
                .join(format!("{}.desktop", package.name)),
        );
        paths.push(
            self.paths
                .integration
                .bash_completions_dir
                .join(&package.name),
        );
        paths.push(
            self.paths
                .integration
                .fish_completions_dir
                .join(format!("{}.fish", package.name)),
        );
        paths.push(
            self.paths
                .integration
                .zsh_completions_dir
                .join(format!("_{}", package.name)),
        );
        estimate_existing_paths(paths)
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
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        self.remove_runtime_integrations(package, message_callback)?;

        if install_path.is_dir() {
            message!(
                message_callback,
                "Removing directory: {}",
                install_path.display()
            );
            fs::remove_dir_all(install_path).context(format!(
                "Failed to remove installation directory at '{}'",
                install_path.display()
            ))?;
        } else if install_path.is_file() {
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
                "Install path '{}' is neither a file nor directory (may have been manually removed)",
                install_path.display()
            ));
        }

        if let Some(icon_path) = &package.icon_path {
            message!(message_callback, "Removing desktop entry ...");
            DesktopManager::remove_entry(self.paths, &package.name).context(format!(
                "Failed to remove desktop entry for '{}'",
                package.name
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

    /// Remove runtime integrations and stored desktop/icon artifacts without touching install_path.
    pub fn remove_runtime_and_desktop_artifacts<H>(
        &self,
        package: &Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        self.remove_runtime_integrations(package, message_callback)?;

        if let Some(icon_path) = &package.icon_path {
            message!(message_callback, "Removing desktop entry ...");
            DesktopManager::remove_entry(self.paths, &package.name).context(format!(
                "Failed to remove desktop entry for '{}'",
                package.name
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
        let install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        if let Some(path_entry) = self.managed_path_entry(package, install_path) {
            message!(
                message_callback,
                "Removing '{}' from PATH ...",
                path_entry.display()
            );

            ShellManager::new(&self.paths.config.paths_file)
                .remove_from_paths(&path_entry)
                .context(format!(
                    "Failed to remove '{}' from PATH configuration",
                    path_entry.display()
                ))?;
        }

        message!(message_callback, "Removing symlink for '{}'", package.name);
        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .remove_link(&package.name)
            .context(format!("Failed to remove symlink for '{}'", package.name))?;

        CompletionManager::new(self.paths)
            .remove_for_package(&package.name, message_callback)
            .context(format!(
                "Failed to remove completion files for '{}'",
                package.name
            ))?;

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
        let install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Package '{}' has no install path recorded", package.name))?;

        if let Some(path_entry) = self.managed_path_entry(package, install_path)
            && path_entry.is_dir()
        {
            message!(
                message_callback,
                "Restoring '{}' to PATH ...",
                path_entry.display()
            );
            ShellManager::new(&self.paths.config.paths_file)
                .add_to_paths(&path_entry)
                .context(format!(
                    "Failed to restore '{}' in PATH configuration",
                    path_entry.display()
                ))?;
        }

        if let Some(exec_path) = package.exec_path.as_ref()
            && exec_path.exists()
        {
            message!(message_callback, "Restoring symlink for '{}'", package.name);
            SymlinkManager::new(&self.paths.integration.symlinks_dir)
                .add_link(exec_path, &package.name)
                .context(format!("Failed to restore symlink for '{}'", package.name))?;
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
        let icons_dir = &self.paths.integration.icons_dir;
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
        if !path.exists() {
            return Ok(());
        }

        if path.is_dir() {
            message!(message_callback, "Purging directory: {}", path.display());
            fs::remove_dir_all(path)
                .context(format!("Failed to remove directory '{}'", path.display()))?;
        } else if path.is_file() {
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
        fs::create_dir_all(&paths.integration.icons_dir).expect("create icons dir");
        let file = paths.integration.icons_dir.join("pkg.png");
        fs::write(&file, b"icon").expect("write icon");
        let nested_dir = root.join("to-remove");
        fs::create_dir_all(nested_dir.join("x")).expect("create nested dir");

        let remover = PackageRemover::new(&paths);
        let mut messages: Option<fn(&str)> = None;
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
    fn remove_runtime_integrations_requires_install_path() {
        let root = temp_root("runtime-missing-path");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.paths_file.parent().expect("parent"))
            .expect("create metadata dir");
        fs::write(&paths.config.paths_file, "").expect("create paths file");

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
        let remover = PackageRemover::new(&paths);
        let mut messages: Option<fn(&str)> = None;

        let err = remover
            .remove_runtime_integrations(&package, &mut messages)
            .expect_err("must fail without install path");
        assert!(err.to_string().contains("no install path"));

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
        package.exec_path = Some(exec_path);
        fs::create_dir_all(&exec_parent).expect("create exec parent");

        assert_eq!(
            PackageRemover::new(&paths).managed_path_entry(&package, &install_path),
            Some(exec_parent)
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn managed_path_entry_uses_recorded_archive_path_even_after_rename() {
        let root = temp_root("path-entry-renamed");
        let paths = test_paths(&root);
        let install_path = paths.install.archives_dir.join("tool");
        let exec_parent = install_path.join("bin");

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
        package.exec_path = Some(exec_parent.join("tool"));

        assert_eq!(
            PackageRemover::new(&paths).managed_path_entry(&package, &install_path),
            Some(exec_parent)
        );
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
        package.exec_path = Some(install_path.clone());
        fs::create_dir_all(install_path.parent().expect("parent")).expect("create parent");
        fs::write(&install_path, b"bin").expect("write binary");

        assert_eq!(
            PackageRemover::new(&paths).managed_path_entry(&package, &install_path),
            None
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn remove_impact_without_purge_reports_removed_active_size() {
        let root = temp_root("impact-no-rollback");
        let paths = test_paths(&root);
        let install_path = paths.install.binaries_dir.join("tool");
        fs::create_dir_all(install_path.parent().expect("parent")).expect("create parent");
        fs::write(&install_path, vec![0_u8; 12]).expect("write binary");

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
        package.exec_path = Some(install_path);

        let impact = PackageRemover::new(&paths).estimate_remove_impact(&package, false);
        assert_eq!(impact.net.bytes, Some(-12));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn remove_impact_with_previous_rollback_still_reports_removed_active_size() {
        let root = temp_root("impact-with-rollback");
        let paths = test_paths(&root);
        let install_path = paths.install.binaries_dir.join("tool");
        let rollback_path = paths.install.rollback_dir.join("tool").join("old-tool");
        fs::create_dir_all(install_path.parent().expect("parent")).expect("create install parent");
        fs::create_dir_all(rollback_path.parent().expect("parent")).expect("create rollback dir");
        fs::write(&install_path, vec![0_u8; 12]).expect("write binary");
        fs::write(&rollback_path, vec![0_u8; 20]).expect("write rollback");

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
        package.exec_path = Some(install_path);

        let impact = PackageRemover::new(&paths).estimate_remove_impact(&package, false);
        assert_eq!(impact.net.bytes, Some(-12));

        cleanup(&root).expect("cleanup");
    }
}
