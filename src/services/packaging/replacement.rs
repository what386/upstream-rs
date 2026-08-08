use anyhow::{Context, Result, anyhow, bail};

use crate::{
    application::cancellation,
    models::upstream::Package,
    services::{
        integration::{CompletionManager, DesktopManager, ShellManager, SymlinkManager},
        packaging::{
            PackagePhase, PackageProgressEvent, PackageRemover, RollbackManager,
            installer::InstallWorkspace,
        },
    },
    storage::{
        database::PackageDatabase,
        rollback::{RollbackSource, RollbackStorage},
    },
    utils::{filesystem::safe_move, static_paths::UpstreamPaths},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

macro_rules! progress {
    ($cb:expr, $event:expr) => {{
        if let Some(cb) = $cb.as_mut() {
            cb($event);
        }
    }};
}

pub(crate) struct PreparedInstall {
    package: Package,
    workspace: InstallWorkspace,
    final_icon_path: Option<PathBuf>,
    staged_desktop_path: Option<PathBuf>,
}

impl PreparedInstall {
    pub(crate) fn new(package: Package, workspace: InstallWorkspace) -> Self {
        Self {
            package,
            workspace,
            final_icon_path: None,
            staged_desktop_path: None,
        }
    }

    fn remap_path(&self, paths: &UpstreamPaths, staged: &Path) -> Result<PathBuf> {
        let relative = staged
            .strip_prefix(self.workspace.root())
            .with_context(|| {
                format!(
                    "Prepared path '{}' is outside replacement payload directory '{}'",
                    staged.display(),
                    self.workspace.root().display()
                )
            })?;
        Ok(paths.dirs.packages_dir.join(relative))
    }

    fn final_package(&self, paths: &UpstreamPaths) -> Result<Package> {
        let mut package = self.package.clone();
        package.install_path = package
            .install_path
            .as_deref()
            .map(|path| self.remap_path(paths, path))
            .transpose()?;
        package.exec_path = package
            .exec_path
            .as_deref()
            .map(|path| self.remap_path(paths, path))
            .transpose()?;
        package.icon_path = self.final_icon_path.clone();
        Ok(package)
    }

    async fn prepare_desktop_data<H>(
        &mut self,
        paths: &UpstreamPaths,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        #[cfg(target_os = "linux")]
        let extractor = crate::services::artifact::AppImageExtractor::new()
            .context("Failed to initialize appimage extractor")?;

        #[cfg(target_os = "linux")]
        let desktop_manager = DesktopManager::new(paths, &extractor);
        #[cfg(not(target_os = "linux"))]
        let desktop_manager = DesktopManager::new(paths);

        let mut final_package = self.final_package(paths)?;
        let desktop_path = self.workspace.desktop_path(&self.package.name);
        desktop_manager
            .prepare_package_entry(
                &self.package,
                &mut final_package,
                &desktop_path,
                self.workspace.icons_dir(),
                message_callback,
            )
            .await
            .context("Failed to prepare desktop integration data")?;
        self.final_icon_path = final_package.icon_path;
        self.staged_desktop_path = Some(desktop_path);

        Ok(())
    }

    fn activate_payload(&self, paths: &UpstreamPaths) -> Result<Package> {
        let staged_install_path = self
            .package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Prepared package has no install path"))?;
        let package = self.final_package(paths)?;
        let final_install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Prepared package has no final install path"))?;
        if let Some(parent) = final_install_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create final install parent '{}'",
                    parent.display()
                )
            })?;
        }
        safe_move::move_file_or_dir(staged_install_path, final_install_path).with_context(
            || {
                format!(
                    "Failed to activate prepared package from '{}' to '{}'",
                    staged_install_path.display(),
                    final_install_path.display()
                )
            },
        )?;
        Ok(package)
    }

    fn install_completions(&self, paths: &UpstreamPaths) -> Result<()> {
        let package_name = &self.package.name;
        let candidates = [
            (
                self.workspace.completions().bash_dir.join(package_name),
                paths.integration.bash_completions_dir.join(package_name),
            ),
            (
                self.workspace
                    .completions()
                    .fish_dir
                    .join(format!("{package_name}.fish")),
                paths
                    .integration
                    .fish_completions_dir
                    .join(format!("{package_name}.fish")),
            ),
            (
                self.workspace
                    .completions()
                    .zsh_dir
                    .join(format!("_{package_name}")),
                paths
                    .integration
                    .zsh_completions_dir
                    .join(format!("_{package_name}")),
            ),
        ];
        for (source, destination) in candidates {
            if !source.exists() {
                continue;
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "Failed to create completion directory '{}'",
                        parent.display()
                    )
                })?;
            }
            safe_move::move_file_or_dir(&source, &destination).with_context(|| {
                format!(
                    "Failed to activate completion '{}' at '{}'",
                    source.display(),
                    destination.display()
                )
            })?;
        }
        Ok(())
    }

    fn install_desktop_artifacts(&self, paths: &UpstreamPaths) -> Result<()> {
        if let Some(staged_icon_path) = self.final_icon_path.as_ref().and_then(|final_icon| {
            final_icon
                .file_name()
                .map(|name| self.workspace.icons_dir().join(name))
        }) && staged_icon_path.exists()
        {
            let final_icon_path = self
                .final_icon_path
                .as_ref()
                .expect("staged icon has a final path");
            if let Some(parent) = final_icon_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create icon directory '{}'", parent.display())
                })?;
            }
            safe_move::move_file_or_dir(&staged_icon_path, final_icon_path).with_context(|| {
                format!(
                    "Failed to activate prepared icon '{}' at '{}'",
                    staged_icon_path.display(),
                    final_icon_path.display()
                )
            })?;
        }

        if let Some(staged_desktop_path) = self.staged_desktop_path.as_ref()
            && staged_desktop_path.exists()
        {
            let destination = DesktopManager::managed_entry_path(paths, &self.package.name);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create desktop directory '{}'", parent.display())
                })?;
            }
            safe_move::move_file_or_dir(staged_desktop_path, &destination).with_context(|| {
                format!(
                    "Failed to activate prepared desktop entry '{}' at '{}'",
                    staged_desktop_path.display(),
                    destination.display()
                )
            })?;
        }
        Ok(())
    }
}

pub(crate) struct ReplacementBackup {
    previous_package: Package,
    partially_installed_package: Option<Package>,
    original_install_path: PathBuf,
    backup_dir: PathBuf,
    package_backup_path: PathBuf,
    integration_destinations: Vec<PathBuf>,
    moved_integrations: Vec<(PathBuf, PathBuf)>,
}

impl ReplacementBackup {
    pub(crate) fn new(
        previous_package: Package,
        original_install_path: PathBuf,
        backup_dir: PathBuf,
    ) -> Result<Self> {
        let file_name = original_install_path
            .file_name()
            .ok_or_else(|| {
                anyhow!(
                    "Install path '{}' has no filename",
                    original_install_path.display()
                )
            })?
            .to_os_string();
        Ok(Self {
            previous_package,
            partially_installed_package: None,
            original_install_path,
            package_backup_path: backup_dir.join("package").join(file_name),
            backup_dir,
            integration_destinations: Vec::new(),
            moved_integrations: Vec::new(),
        })
    }

    fn move_package(&self) -> Result<()> {
        let parent = self
            .package_backup_path
            .parent()
            .expect("package backup path has a parent");
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create package backup directory '{}'",
                parent.display()
            )
        })?;
        safe_move::move_file_or_dir(&self.original_install_path, &self.package_backup_path)
    }

    fn move_integration(&mut self, original: PathBuf, category: &str) -> Result<()> {
        self.integration_destinations.push(original.clone());
        match fs::symlink_metadata(&original) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Failed to inspect integration '{}'", original.display())
                });
            }
        }
        let file_name = original
            .file_name()
            .ok_or_else(|| anyhow!("Integration path '{}' has no filename", original.display()))?;
        let backup = self.backup_dir.join(category).join(file_name);
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create integration backup directory '{}'",
                    parent.display()
                )
            })?;
        }
        safe_move::move_file_or_dir(&original, &backup)?;
        self.moved_integrations.push((original, backup));
        Ok(())
    }

    pub(crate) fn move_integrations(&mut self, paths: &UpstreamPaths) -> Result<()> {
        for path in
            CompletionManager::new(paths).package_completion_paths(&self.previous_package.name)
        {
            self.move_integration(path, "completions")?;
        }
        self.move_integration(
            DesktopManager::managed_entry_path(paths, &self.previous_package.name),
            "desktop",
        )?;
        if let Some(icon_path) = self.previous_package.icon_path.clone() {
            self.move_integration(icon_path, "icons")?;
        }
        Ok(())
    }

    pub(crate) fn set_partial_package(&mut self, package: Package) {
        self.partially_installed_package = Some(package);
    }

    fn rollback_icon_path(&self) -> Option<&Path> {
        let original_icon = self.previous_package.icon_path.as_ref()?;
        self.moved_integrations
            .iter()
            .find(|(original, _)| original == original_icon)
            .map(|(_, stored)| stored.as_path())
    }
}

pub struct PackageReplacer<'a> {
    paths: &'a UpstreamPaths,
}

impl<'a> PackageReplacer<'a> {
    pub fn new(paths: &'a UpstreamPaths) -> Self {
        Self { paths }
    }

    pub(crate) async fn install_new<H, P>(
        &self,
        package_database: &mut PackageDatabase,
        prepared: PreparedInstall,
        add_entry: bool,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        self.install_new_with_settings(
            package_database,
            prepared,
            add_entry,
            None,
            message_callback,
            progress_callback,
        )
        .await
    }

    pub(crate) async fn install_new_with_settings<H, P>(
        &self,
        package_database: &mut PackageDatabase,
        mut prepared: PreparedInstall,
        add_entry: bool,
        trust_mode: Option<crate::models::common::enums::TrustMode>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        cancellation::check()?;
        if add_entry {
            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::CreatingDesktopEntry)
            );
            prepared
                .prepare_desktop_data(self.paths, message_callback)
                .await?;
        }

        let updated_package = prepared.activate_payload(self.paths)?;
        if let Some(exec_path) = updated_package.exec_path.as_ref() {
            SymlinkManager::new(&self.paths.state.symlinks_dir)
                .add_link(exec_path, &updated_package.name)
                .context("Failed to activate runtime link")?;
        }
        prepared.install_completions(self.paths)?;
        if add_entry {
            prepared.install_desktop_artifacts(self.paths)?;
        }

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::SavingMetadata)
        );
        let persist_result = if trust_mode.is_some() {
            let mut settings =
                crate::storage::database::PackageSettings::new(&updated_package.name);
            settings.trust_mode = trust_mode;
            package_database.upsert_package_with_settings(&updated_package, &settings)
        } else {
            package_database.upsert_package(&updated_package)
        };
        if let Err(error) = persist_result {
            let _ = PackageRemover::new(self.paths)
                .remove_package_files(&updated_package, &mut None::<fn(&str)>);
            return Err(error).context(format!(
                "Failed to save package '{}' to storage",
                updated_package.name
            ));
        }
        if let Err(error) = ShellManager::new(&self.paths.generated.paths_file)
            .regenerate_paths(package_database, self.paths)
        {
            let _ = package_database.remove_package(&updated_package.name);
            let _ = PackageRemover::new(self.paths)
                .remove_package_files(&updated_package, &mut None::<fn(&str)>);
            return Err(error).context(format!(
                "Failed to refresh shell PATH after installing '{}'",
                updated_package.name
            ));
        }

        Ok(updated_package)
    }

    pub(crate) async fn replace<H, P>(
        &self,
        previous_package: &Package,
        mut prepared: PreparedInstall,
        restore_desktop: bool,
        rollback_source: RollbackSource,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        cancellation::check()?;
        if restore_desktop {
            prepared
                .prepare_desktop_data(self.paths, message_callback)
                .await?;
        }
        cancellation::check()?;
        let original_install_path = previous_package
            .install_path
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "Package '{}' has no install path recorded",
                    previous_package.name
                )
            })?
            .clone();
        let updated_package = prepared.final_package(self.paths)?;
        let final_install_path = updated_package
            .install_path
            .as_ref()
            .ok_or_else(|| anyhow!("Prepared replacement has no final install path"))?;
        if final_install_path != &original_install_path
            && fs::symlink_metadata(final_install_path).is_ok()
        {
            bail!(
                "Prepared replacement path '{}' already exists",
                final_install_path.display()
            );
        }

        let backup_dir = Self::backup_dir(self.paths, &previous_package.name)?;
        let mut backup = ReplacementBackup::new(
            previous_package.clone(),
            original_install_path.clone(),
            backup_dir.clone(),
        )?;

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot)
        );
        backup.move_package().with_context(|| {
            format!(
                "Failed to move active package '{}' into transient snapshot '{}'",
                original_install_path.display(),
                backup_dir.display()
            )
        })?;
        backup.set_partial_package(updated_package.clone());
        if let Err(error) = backup.move_integrations(self.paths) {
            return self.restore_after_failure(
                backup,
                error,
                "Failed to move existing integrations into transient snapshot",
                message_callback,
            );
        }

        if cancellation::is_requested() {
            return self.restore_after_failure(
                backup,
                anyhow!("Operation interrupted by CTRL-C"),
                "Replacement interrupted",
                message_callback,
            );
        }

        let updated_package = match prepared.activate_payload(self.paths) {
            Ok(package) => package,
            Err(error) => {
                return self.restore_after_failure(
                    backup,
                    error,
                    "Failed to activate prepared replacement",
                    message_callback,
                );
            }
        };
        backup.set_partial_package(updated_package.clone());

        if let Some(exec_path) = updated_package.exec_path.as_ref()
            && let Err(error) = SymlinkManager::new(&self.paths.state.symlinks_dir)
                .add_link(exec_path, &updated_package.name)
        {
            return self.restore_after_failure(
                backup,
                error.context("Failed to activate runtime link"),
                "Failed to activate prepared replacement",
                message_callback,
            );
        }
        if let Err(error) = prepared.install_completions(self.paths) {
            return self.restore_after_failure(
                backup,
                error,
                "Failed to activate prepared completions",
                message_callback,
            );
        }

        if restore_desktop {
            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::CreatingRuntimeLinks)
            );
            if let Err(error) = prepared.install_desktop_artifacts(self.paths) {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Phase(PackagePhase::RollingBack)
                );
                return self.restore_after_failure(
                    backup,
                    error.context("Failed to activate prepared desktop integration"),
                    "Failed to activate prepared desktop integration",
                    message_callback,
                );
            }
            backup.set_partial_package(updated_package.clone());
        }

        if let Err(error) = Self::capture_rollback(
            self.paths,
            previous_package,
            &backup.package_backup_path,
            backup.rollback_icon_path(),
            rollback_source,
        ) {
            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::RollingBack)
            );
            return self.restore_after_failure(
                backup,
                error.context(format!(
                    "Failed to capture rollback for '{}'",
                    previous_package.name
                )),
                "Failed to finalize replacement",
                message_callback,
            );
        }

        if let Err(error) = Self::remove_path_if_exists(&backup_dir) {
            progress!(
                progress_callback,
                PackageProgressEvent::Warning(format!(
                    "Replacement succeeded, but transient backup '{}' could not be removed: {error:#}",
                    backup_dir.display()
                ))
            );
        }
        Ok(updated_package)
    }

    fn backup_dir(paths: &UpstreamPaths, package_name: &str) -> Result<PathBuf> {
        fs::create_dir_all(&paths.install.tmp_dir).context(format!(
            "Failed to create replacement temp directory '{}'",
            paths.install.tmp_dir.display()
        ))?;
        let backup_dir = paths.install.tmp_dir.join(format!("{package_name}.old"));
        if fs::symlink_metadata(&backup_dir).is_ok() {
            bail!(
                "Transient snapshot '{}' already exists",
                backup_dir.display()
            );
        }
        Ok(backup_dir)
    }

    pub(crate) fn remove_path_if_exists(path: &Path) -> Result<()> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).context(format!("Failed to inspect path '{}'", path.display()));
            }
        };
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(path)
                .context(format!("Failed to remove directory '{}'", path.display()))?;
        } else {
            fs::remove_file(path).context(format!("Failed to remove file '{}'", path.display()))?;
        }
        Ok(())
    }

    pub(crate) fn capture_rollback(
        paths: &UpstreamPaths,
        package: &Package,
        backup_path: &Path,
        icon_source: Option<&Path>,
        source: RollbackSource,
    ) -> Result<()> {
        let rollback_file = RollbackManager::rollback_file_path(paths);
        let mut rollback_storage = RollbackStorage::new(&rollback_file)?;
        RollbackManager::capture_backup_path_with_icon_source(
            paths,
            &mut rollback_storage,
            package,
            backup_path,
            icon_source,
            source,
            &mut None::<fn(&str)>,
        )
    }

    pub(crate) fn restore_after_failure<H, T>(
        &self,
        backup: ReplacementBackup,
        failure: anyhow::Error,
        failure_context: &'static str,
        message_callback: &mut Option<H>,
    ) -> Result<T>
    where
        H: FnMut(&str),
    {
        let mut errors = Vec::new();
        let replacement_path = backup
            .partially_installed_package
            .as_ref()
            .and_then(|package| package.install_path.as_deref())
            .unwrap_or(&backup.original_install_path);
        if let Err(error) = Self::remove_path_if_exists(replacement_path) {
            errors.push(format!(
                "failed to remove replacement '{}': {error:#}",
                replacement_path.display()
            ));
        }

        if fs::symlink_metadata(&backup.original_install_path).is_err() {
            if let Err(error) = safe_move::move_file_or_dir(
                &backup.package_backup_path,
                &backup.original_install_path,
            ) {
                errors.push(format!("failed to restore active package: {error:#}"));
            }
        } else if fs::symlink_metadata(&backup.package_backup_path).is_ok() {
            errors.push(format!(
                "cannot restore '{}' because the destination still exists",
                backup.original_install_path.display()
            ));
        }

        if let Err(error) = PackageRemover::new(self.paths)
            .restore_runtime_integrations(&backup.previous_package, message_callback)
        {
            errors.push(format!("failed to restore runtime integrations: {error:#}"));
        }
        let mut integration_destinations = backup.integration_destinations.clone();
        if let Some(icon_path) = backup
            .partially_installed_package
            .as_ref()
            .and_then(|package| package.icon_path.clone())
        {
            integration_destinations.push(icon_path);
        }
        for destination in integration_destinations {
            if let Err(error) = Self::remove_path_if_exists(&destination) {
                errors.push(format!(
                    "failed to remove replacement integration '{}': {error:#}",
                    destination.display()
                ));
            }
        }
        for (original, stored) in backup.moved_integrations.iter().rev() {
            if let Some(parent) = original.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                errors.push(format!(
                    "failed to recreate integration directory '{}': {error:#}",
                    parent.display()
                ));
                continue;
            }
            if let Err(error) = safe_move::move_file_or_dir(stored, original) {
                errors.push(format!(
                    "failed to restore integration '{}': {error:#}",
                    original.display()
                ));
            }
        }
        if let Err(error) = Self::remove_path_if_exists(&backup.backup_dir) {
            errors.push(format!(
                "failed to remove transient snapshot '{}': {error:#}",
                backup.backup_dir.display()
            ));
        }

        if !errors.is_empty() {
            return Err(anyhow!(
                "{} for '{}': {failure:#}. Rollback encountered: {}",
                failure_context,
                backup.previous_package.name,
                errors.join("; ")
            ));
        }

        Err(failure).context(format!(
            "{} for '{}' (previous version restored)",
            failure_context, backup.previous_package.name
        ))
    }

    pub(crate) fn persist(
        &self,
        package_database: &mut PackageDatabase,
        replacement: &Package,
    ) -> Result<()> {
        if let Err(persistence_error) = package_database.upsert_package(replacement) {
            return self.rollback_failed_database_commit(
                package_database,
                replacement,
                persistence_error,
            );
        }

        ShellManager::new(&self.paths.generated.paths_file)
            .regenerate_paths(package_database, self.paths)
            .context(format!(
                "Replacement for '{}' was persisted, but shell PATH files could not be refreshed",
                replacement.name
            ))
    }

    fn rollback_failed_database_commit(
        &self,
        package_database: &mut PackageDatabase,
        replacement: &Package,
        persistence_error: anyhow::Error,
    ) -> Result<()> {
        let rollback_result = (|| {
            let rollback_file = RollbackManager::rollback_file_path(self.paths);
            let mut rollback_storage = RollbackStorage::new(&rollback_file)?;
            RollbackManager::new(self.paths, package_database, &mut rollback_storage)
                .restore_replaced_package(&replacement.name, replacement, &mut None::<fn(&str)>)
        })();

        match rollback_result {
            Ok(()) => Err(persistence_error).context(format!(
                "Failed to persist replacement for '{}' (previous version restored)",
                replacement.name
            )),
            Err(rollback_error) => Err(anyhow!(
                "Failed to persist replacement for '{}': {}. Rollback also failed: {}",
                replacement.name,
                persistence_error,
                rollback_error
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InstallWorkspace, PackageReplacer, PreparedInstall};
    use crate::{
        models::{
            common::{
                Version,
                enums::{Channel, Filetype, Provider, TrustMode},
            },
            upstream::Package,
        },
        services::{integration::SymlinkManager, packaging::RollbackManager},
        storage::{
            database::{PackageDatabase, PackageSettings},
            rollback::{RollbackSource, RollbackStorage},
        },
        utils::test_support,
    };
    use rusqlite::Connection;
    use std::fs;

    #[tokio::test]
    async fn staged_new_install_activates_before_persisting_metadata() {
        let root = test_support::temp_root("upstream-package-replacement-test", "new-install");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create temp");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks");
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config");

        let workspace = InstallWorkspace::new(&paths, "tool").expect("create workspace");
        let staged_path = workspace.root().join("binaries/tool");
        fs::create_dir_all(staged_path.parent().expect("staged parent"))
            .expect("create staged binaries");
        fs::write(&staged_path, b"new binary").expect("write staged binary");

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
        package.install_path = Some(staged_path.clone());
        package.exec_path = Some(staged_path);

        let mut database = PackageDatabase::open(&paths.metadata.packages_database_file)
            .expect("open package database");
        let updated = PackageReplacer::new(&paths)
            .install_new(
                &mut database,
                PreparedInstall::new(package, workspace),
                false,
                &mut None::<fn(&str)>,
                &mut None::<fn(crate::services::packaging::PackageProgressEvent)>,
            )
            .await
            .expect("activate new package");

        let active_path = paths.install.binaries_dir.join("tool");
        assert_eq!(updated.install_path.as_deref(), Some(active_path.as_path()));
        assert_eq!(
            fs::read(&active_path).expect("read active binary"),
            b"new binary"
        );
        assert!(
            database
                .get_package("tool")
                .expect("read package")
                .is_some()
        );
        #[cfg(windows)]
        assert!(paths.state.symlinks_dir.join("tool.exe").exists());
        #[cfg(not(windows))]
        assert!(paths.state.symlinks_dir.join("tool").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[tokio::test]
    async fn staged_replacement_keeps_old_active_until_cutover_then_removes_backup() {
        let root = test_support::temp_root("upstream-package-replacement-test", "staged-cutover");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create temp");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks");
        fs::create_dir_all(&paths.state.icons_dir).expect("create icons");
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config");
        fs::write(
            &paths.config.config_file,
            "[rollback]\ncompression_level = \"none\"\nstored_artifacts = 1\n",
        )
        .expect("write rollback config");

        let install_path = paths.install.binaries_dir.join("tool");
        fs::write(&install_path, b"old binary").expect("write active binary");
        let mut previous = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        previous.install_path = Some(install_path.clone());
        previous.exec_path = Some(install_path.clone());
        let old_icon = paths.state.icons_dir.join("tool.png");
        fs::write(&old_icon, b"old icon").expect("write old icon");
        previous.icon_path = Some(old_icon.clone());

        let workspace = InstallWorkspace::new(&paths, "tool").expect("create workspace");
        let staged_path = workspace.root().join("binaries/tool");
        fs::create_dir_all(staged_path.parent().expect("staged parent"))
            .expect("create staged binaries");
        fs::write(&staged_path, b"new binary").expect("write staged binary");
        let mut candidate = previous.clone();
        candidate.install_path = Some(staged_path.clone());
        candidate.exec_path = Some(staged_path);

        assert_eq!(
            fs::read(&install_path).expect("read active before cutover"),
            b"old binary"
        );
        let updated = PackageReplacer::new(&paths)
            .replace(
                &previous,
                PreparedInstall::new(candidate, workspace),
                false,
                RollbackSource::Upgrade,
                &mut None::<fn(&str)>,
                &mut None::<fn(crate::services::packaging::PackageProgressEvent)>,
            )
            .await
            .expect("replace staged package");

        assert_eq!(
            updated.install_path.as_deref(),
            Some(install_path.as_path())
        );
        assert_eq!(
            fs::read(&install_path).expect("read active after cutover"),
            b"new binary"
        );
        assert!(
            fs::read_dir(&paths.install.tmp_dir)
                .expect("read temp")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".old"))
        );
        let rollback_file = RollbackManager::rollback_file_path(&paths);
        let rollback_storage = RollbackStorage::new(&rollback_file).expect("open rollback storage");
        let rollback_record = rollback_storage
            .get_record("tool")
            .expect("rollback record");
        let rollback_icon = paths.state.rollback_dir.join(
            rollback_record
                .icon_relative_path
                .as_ref()
                .expect("rollback icon"),
        );
        assert_eq!(
            fs::read(rollback_icon).expect("read rollback icon"),
            b"old icon"
        );
        assert!(!old_icon.exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[tokio::test]
    async fn failed_candidate_activation_restores_old_without_retaining_backup() {
        let root = test_support::temp_root("upstream-package-replacement-test", "failed-cutover");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create temp");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks");

        let install_path = paths.install.binaries_dir.join("tool");
        fs::write(&install_path, b"old binary").expect("write active binary");
        let mut previous = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        previous.install_path = Some(install_path.clone());
        previous.exec_path = Some(install_path.clone());

        let workspace = InstallWorkspace::new(&paths, "tool").expect("create workspace");
        let missing_staged_path = workspace.root().join("binaries/tool");
        let mut candidate = previous.clone();
        candidate.install_path = Some(missing_staged_path.clone());
        candidate.exec_path = Some(missing_staged_path);

        let error = PackageReplacer::new(&paths)
            .replace(
                &previous,
                PreparedInstall::new(candidate, workspace),
                false,
                RollbackSource::Upgrade,
                &mut None::<fn(&str)>,
                &mut None::<fn(crate::services::packaging::PackageProgressEvent)>,
            )
            .await
            .expect_err("missing staged payload should fail activation");

        assert!(error.to_string().contains("previous version restored"));
        assert_eq!(
            fs::read(&install_path).expect("read restored binary"),
            b"old binary"
        );
        assert!(
            fs::read_dir(&paths.install.tmp_dir)
                .expect("read temp")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".old"))
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn failed_database_commit_restores_previous_files_and_metadata() {
        let root = test_support::temp_root("upstream-package-replacement-test", "rollback");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");

        let install_path = paths.install.binaries_dir.join("tool");
        let backup_path = paths.install.tmp_dir.join("tool.old");
        fs::write(&backup_path, b"old binary").expect("write backup");

        let mut previous = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        previous.version = Version::new(1, 0, 0, false);
        previous.install_path = Some(install_path.clone());
        previous.exec_path = Some(install_path.clone());

        let mut database =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open database");
        let mut settings = PackageSettings::new("tool");
        settings.trust_mode = Some(TrustMode::Signature);
        database
            .upsert_package_with_settings(&previous, &settings)
            .expect("store previous package and settings");
        let rollback_file = RollbackManager::rollback_file_path(&paths);
        let mut rollback_storage =
            RollbackStorage::new(&rollback_file).expect("open rollback storage");
        RollbackManager::capture_backup_path(
            &paths,
            &mut rollback_storage,
            &previous,
            &backup_path,
            RollbackSource::Upgrade,
            &mut None::<fn(&str)>,
        )
        .expect("capture rollback");

        fs::write(&install_path, b"new binary").expect("write replacement");
        SymlinkManager::new(&paths.state.symlinks_dir)
            .add_link(&install_path, "tool")
            .expect("create replacement link");
        let mut replacement = previous.clone();
        replacement.version = Version::new(2, 0, 0, false);

        let database_path =
            PackageDatabase::database_path_for(&paths.metadata.packages_database_file);
        Connection::open(&database_path)
            .expect("open trigger connection")
            .execute_batch(
                "
                CREATE TRIGGER reject_v2
                BEFORE UPDATE ON packages
                WHEN NEW.version_major = 2
                BEGIN
                    SELECT RAISE(ABORT, 'reject test replacement');
                END;
                ",
            )
            .expect("create trigger");

        let error = PackageReplacer::new(&paths)
            .persist(&mut database, &replacement)
            .expect_err("replacement persistence should fail");

        assert!(error.to_string().contains("previous version restored"));
        assert_eq!(
            fs::read(&install_path).expect("read restored binary"),
            b"old binary"
        );
        assert_eq!(
            database
                .get_package("tool")
                .expect("load package")
                .expect("stored package")
                .version,
            Version::new(1, 0, 0, false)
        );
        assert_eq!(
            database
                .get_package_settings("tool")
                .expect("load settings")
                .expect("stored settings")
                .trust_mode,
            Some(TrustMode::Signature)
        );
        assert!(
            RollbackStorage::new(&rollback_file)
                .expect("reload rollback storage")
                .get_record("tool")
                .is_none()
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn shell_refresh_failure_keeps_persisted_replacement_consistent() {
        let root = test_support::temp_root("upstream-package-replacement-test", "shell-failure");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.dirs.data_dir).expect("create data");
        fs::write(&paths.dirs.generated_dir, b"blocks generated directory")
            .expect("create blocking file");

        let install_path = paths.install.binaries_dir.join("tool");
        fs::write(&install_path, b"new binary").expect("write replacement");
        let mut previous = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        previous.version = Version::new(1, 0, 0, false);
        previous.install_path = Some(install_path.clone());
        previous.exec_path = Some(install_path.clone());
        let mut replacement = previous.clone();
        replacement.version = Version::new(2, 0, 0, false);

        let mut database =
            PackageDatabase::open(&paths.metadata.packages_database_file).expect("open database");
        database
            .upsert_package(&previous)
            .expect("store previous package");

        let error = PackageReplacer::new(&paths)
            .persist(&mut database, &replacement)
            .expect_err("shell refresh should fail");

        assert!(error.to_string().contains("was persisted"));
        assert_eq!(
            database
                .get_package("tool")
                .expect("load package")
                .expect("stored package")
                .version,
            Version::new(2, 0, 0, false)
        );
        assert_eq!(
            fs::read(&install_path).expect("read replacement"),
            b"new binary"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
