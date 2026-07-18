use crate::{
    application::cancellation,
    models::{
        common::enums::TrustMode,
        provider::{Asset, Release},
        upstream::{InstallType, Package},
    },
    output,
    providers::provider_manager::ProviderManager,
    routines::build::{BuildRequest, scripts::BuildScriptAction, worker::BuildWorker},
    services::{
        artifact::zsync_handler,
        integration::{CompletionManager, DesktopManager, SymlinkManager},
        packaging::RollbackManager,
        packaging::{PackageInstaller, PackagePhase, PackageProgressEvent, PackageRemover},
        trust::TrustedSignatureKeys,
    },
    storage::rollback::{RollbackSource, RollbackStorage},
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, bail};
use console::style;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

macro_rules! progress {
    ($cb:expr, $event:expr) => {{
        if let Some(cb) = $cb.as_mut() {
            cb($event);
        }
    }};
}

pub struct PackageUpgrader<'a> {
    provider_manager: &'a ProviderManager,
    installer: PackageInstaller<'a>,
    remover: PackageRemover<'a>,
    paths: &'a UpstreamPaths,
    trusted_keys: TrustedSignatureKeys,
}

#[derive(Clone)]
pub enum ResolvedUpgradeTarget {
    Release(Release),
    Branch { branch: String, head_commit: String },
}

struct UpgradeRollbackGuard {
    previous_package: Package,
    partially_installed_package: Option<Package>,
    original_install_path: PathBuf,
    backup_path: PathBuf,
    symlinks_dir: Option<PathBuf>,
    armed: bool,
}

impl UpgradeRollbackGuard {
    fn new(
        previous_package: Package,
        original_install_path: PathBuf,
        backup_path: PathBuf,
    ) -> Self {
        Self {
            previous_package,
            partially_installed_package: None,
            original_install_path,
            backup_path,
            symlinks_dir: None,
            armed: true,
        }
    }

    fn attach_paths(&mut self, paths: &UpstreamPaths) {
        self.symlinks_dir = Some(paths.state.symlinks_dir.clone());
    }

    fn set_partial_package(&mut self, package: Package) {
        self.partially_installed_package = Some(package);
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for UpgradeRollbackGuard {
    fn drop(&mut self) {
        if !self.armed || !self.backup_path.exists() {
            return;
        }

        // This is the cancellation fallback. The normal error path below also
        // restores integrations and reports detailed errors; Drop must remain
        // best-effort because it cannot return a Result.
        let _ = if let Some(partial) = &self.partially_installed_package {
            // The normal error path performs the full package cleanup. Here
            // we only need to remove the replacement path before restoring
            // the snapshot.
            PackageUpgrader::remove_path_if_exists(
                partial
                    .install_path
                    .as_deref()
                    .unwrap_or(&self.original_install_path),
            )
        } else {
            PackageUpgrader::remove_path_if_exists(&self.original_install_path)
        };
        let _ = fs::rename(&self.backup_path, &self.original_install_path);
        if let (Some(symlinks_dir), Some(exec_path)) =
            (&self.symlinks_dir, self.previous_package.exec_path.as_ref())
        {
            let _ =
                SymlinkManager::new(symlinks_dir).add_link(exec_path, &self.previous_package.name);
        }
    }
}

impl<'a> PackageUpgrader<'a> {
    fn zsync_work_root(paths: &UpstreamPaths, package: &Package) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        paths
            .install
            .tmp_dir
            .join(format!("upstream-zsync-{}-{nonce}", package.name))
    }

    fn backup_path(paths: &UpstreamPaths, install_path: &Path) -> Result<PathBuf> {
        let file_name = install_path.file_name().ok_or_else(|| {
            anyhow::anyhow!("Install path '{}' has no filename", install_path.display())
        })?;
        fs::create_dir_all(&paths.install.tmp_dir).context(format!(
            "Failed to create upgrade temp directory '{}'",
            paths.install.tmp_dir.display()
        ))?;
        Ok(paths
            .install
            .tmp_dir
            .join(format!("{}.old", file_name.to_string_lossy())))
    }

    fn remove_path_if_exists(path: &Path) -> Result<()> {
        if path.is_dir() {
            fs::remove_dir_all(path)
                .context(format!("Failed to remove directory '{}'", path.display()))?;
        } else if path.is_file() {
            fs::remove_file(path).context(format!("Failed to remove file '{}'", path.display()))?;
        }
        Ok(())
    }

    fn can_apply_zsync(package: &Package, asset: &Asset, install_path: &Path) -> bool {
        install_path.is_file()
            && asset.filetype == package.filetype
            && !matches!(
                package.filetype,
                crate::models::common::enums::Filetype::Archive
                    | crate::models::common::enums::Filetype::Compressed
                    | crate::models::common::enums::Filetype::MacApp
                    | crate::models::common::enums::Filetype::MacDmg
            )
    }

    #[allow(clippy::too_many_arguments)]
    async fn try_zsync_upgrade_release<F, H, P>(
        &self,
        package: Package,
        release: &Release,
        asset: &Asset,
        trust_mode: TrustMode,
        backup_path: &Path,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Option<Package>>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        if !Self::can_apply_zsync(&package, asset, backup_path) {
            return Ok(None);
        }

        if zsync_handler::find_asset(release, asset).is_none() {
            return Ok(None);
        }

        let work_root = Self::zsync_work_root(self.paths, &package);
        let work_cache = work_root.join("downloads");
        fs::create_dir_all(&work_cache).context(format!(
            "Failed to create zsync cache directory '{}'",
            work_cache.display()
        ))?;

        let work_target =
            work_root.join(Path::new(&asset.name).file_name().ok_or_else(|| {
                anyhow::anyhow!("Selected asset '{}' has no filename", asset.name)
            })?);

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::ApplyingZsyncUpdate)
        );
        {
            let progress_cell = std::cell::RefCell::new(progress_callback.as_mut());
            let mut zsync_progress = Some(|downloaded: u64, total: u64| {
                if let Some(cb) = progress_cell.borrow_mut().as_deref_mut() {
                    cb(PackageProgressEvent::Zsync { downloaded, total });
                }
            });
            if let Err(err) = zsync_handler::update_selected_asset(
                &package,
                release,
                asset,
                self.provider_manager,
                &work_cache,
                backup_path,
                &work_target,
                message_callback.as_mut(),
                &mut zsync_progress,
            )
            .await
            .context(format!("Failed to update '{}' via zsync", package.name))
            {
                let _ = fs::remove_dir_all(&work_root);
                return Err(err);
            }
        }

        let result: Result<Package> = async {
            let trust_verifier = crate::services::trust::TrustVerifier::new(
                self.provider_manager,
                &work_cache,
                trust_mode,
                &self.trusted_keys,
            );
            let mut verifier_download_progress: Option<fn(u64, u64)> = None;
            trust_verifier
                .verify_file(
                    &work_target,
                    release,
                    &package.provider,
                    &mut verifier_download_progress,
                    message_callback,
                    progress_callback,
                )
                .await
                .context("Failed trust verification for zsync-updated artifact")?;

            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::InstallingCompletions)
            );
            if let Err(err) = CompletionManager::new(self.paths)
                .install_from_release_assets(
                    &package.name,
                    release,
                    self.provider_manager,
                    &package.provider,
                    &work_cache,
                    message_callback,
                )
                .await
            {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Warning(format!("Completion install skipped: {err}"))
                );
            }

            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::InstallingPackage)
            );
            let mut install_pkg = package;
            install_pkg.install_path = None;
            install_pkg.exec_path = None;
            install_pkg.version = release.version.clone();
            install_pkg.version_tag_template =
                Package::version_tag_template_from_tag(&release.tag, &release.version);

            self.installer
                .install_local_artifact_files(
                    install_pkg,
                    &work_target,
                    release.version.clone(),
                    message_callback,
                )
                .context("Failed to install zsync-updated artifact")
        }
        .await;

        if let Some(callback) = download_progress.as_mut() {
            callback(0, 0);
        }

        let _ = fs::remove_dir_all(&work_root);
        result.map(Some)
    }

    fn capture_successful_upgrade_rollback(
        paths: &UpstreamPaths,
        package: &Package,
        backup_path: &Path,
    ) -> Result<()> {
        let rollback_file = RollbackManager::rollback_file_path(paths);
        let mut rollback_storage = RollbackStorage::new(&rollback_file)?;
        RollbackManager::capture_backup_path(
            paths,
            &mut rollback_storage,
            package,
            backup_path,
            RollbackSource::Upgrade,
            &mut None::<fn(&str)>,
        )
    }

    pub fn new(
        provider_manager: &'a ProviderManager,
        installer: PackageInstaller<'a>,
        remover: PackageRemover<'a>,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
    ) -> Self {
        Self {
            provider_manager,
            installer,
            remover,
            paths,
            trusted_keys,
        }
    }

    /// Upgrade a single package.
    ///
    /// Returns:
    /// - Ok(None) => no upgrade needed
    /// - Ok(Some(Package)) => upgraded package
    pub async fn upgrade<F, H>(
        &self,
        package: &Package,
        force: bool,
        trust_mode: TrustMode,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<Option<Package>>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        if package.is_pinned {
            message!(
                message_callback,
                "Upgrade skipped: '{}' is pinned",
                package.name
            );
            return Ok(None);
        }

        let target = if package.install_type == InstallType::Build && package.build_branch.is_some()
        {
            None
        } else {
            message!(message_callback, "Fetching latest release ...");
            let release = if force {
                self.provider_manager
                    .get_latest_release(
                        &package.repo_slug,
                        &package.provider,
                        &package.channel,
                        package.base_url.as_deref(),
                    )
                    .await
                    .context(format!("fetch latest release for '{}'", package.name))?
            } else {
                let Some(latest_release) =
                    self.provider_manager
                        .check_for_updates(package)
                        .await
                        .context(format!("fetch latest release for '{}'", package.name))?
                else {
                    message!(message_callback, "'{}' is already up to date", package.name);
                    return Ok(None);
                };

                latest_release
            };

            if !force && !package.is_update_available(&release) {
                message!(message_callback, "'{}' is already up to date", package.name);
                return Ok(None);
            }
            Some(ResolvedUpgradeTarget::Release(release))
        };

        if package.install_type == InstallType::Build
            && let Some(branch) = package.build_branch.as_deref()
        {
            let head_commit = self
                .provider_manager
                .get_branch_head_sha(
                    &package.repo_slug,
                    &package.provider,
                    branch,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch branch head for '{}' on '{}'",
                    branch, package.name
                ))?;
            let up_to_date = package
                .build_commit
                .as_deref()
                .is_some_and(|saved| saved == head_commit);
            if up_to_date && !force {
                message!(
                    message_callback,
                    "'{}' is already up to date (branch '{}')",
                    package.name,
                    branch
                );
                return Ok(None);
            }
            let mut no_progress: Option<fn(PackageProgressEvent)> = None;
            return self
                .upgrade_resolved(
                    package,
                    ResolvedUpgradeTarget::Branch {
                        branch: branch.to_string(),
                        head_commit,
                    },
                    trust_mode,
                    download_progress,
                    message_callback,
                    &mut no_progress,
                )
                .await
                .map(Some);
        }

        let Some(target) = target else {
            return Ok(None);
        };

        let mut no_progress: Option<fn(PackageProgressEvent)> = None;
        self.upgrade_resolved(
            package,
            target,
            trust_mode,
            download_progress,
            message_callback,
            &mut no_progress,
        )
        .await
        .map(Some)
    }

    pub async fn upgrade_resolved<F, H, P>(
        &self,
        package: &Package,
        target: ResolvedUpgradeTarget,
        trust_mode: TrustMode,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        cancellation::check()?;
        if package.is_pinned {
            bail!("Package '{}' is pinned", package.name);
        }

        let had_desktop_integration = package.icon_path.is_some();

        message!(
            message_callback,
            "{}",
            style(format!("Upgrading '{}' ...", package.name)).cyan()
        );

        let original_install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!("Package '{}' has no install path recorded", package.name)
            })?
            .clone();
        let backup_path = Self::backup_path(self.paths, &original_install_path)?;
        let mut rollback_guard = UpgradeRollbackGuard::new(
            package.clone(),
            original_install_path.clone(),
            backup_path.clone(),
        );
        rollback_guard.attach_paths(self.paths);

        Self::remove_path_if_exists(&backup_path)?;

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot)
        );
        fs::rename(&original_install_path, &backup_path).context(format!(
            "Failed to back up '{}' to '{}'",
            original_install_path.display(),
            backup_path.display()
        ))?;
        cancellation::check()?;

        // Remove runtime integrations (PATH/symlink) but keep desktop assets
        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::RemovingRuntimeLinks)
        );
        if let Err(e) = self
            .remover
            .remove_runtime_integrations(package, message_callback)
        {
            let _ = fs::rename(&backup_path, &original_install_path);
            let _ = self
                .remover
                .restore_runtime_integrations(package, message_callback);
            return Err(e).context(format!(
                "Failed to remove runtime integration for '{}'",
                package.name
            ));
        }

        // Install new version

        if package.install_type == InstallType::Build {
            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::RebuildingFromSource)
            );
            let (version_tag, branch, branch_head_commit) = match &target {
                ResolvedUpgradeTarget::Release(release) => (Some(release.tag.clone()), None, None),
                ResolvedUpgradeTarget::Branch {
                    branch,
                    head_commit,
                } => (None, Some(branch.clone()), Some(head_commit.clone())),
            };
            let worker = BuildWorker::new(self.provider_manager, self.paths);
            let build_result = {
                let mut build_line_callback = Some(|line: &str| {
                    let line = line.trim();
                    if !line.is_empty() {
                        progress!(
                            progress_callback,
                            PackageProgressEvent::Warning(line.to_string())
                        );
                    }
                });
                worker
                    .build(
                        BuildRequest {
                            name: package.name.clone(),
                            repo_slug: package.repo_slug.clone(),
                            provider: package.provider.clone(),
                            base_url: package.base_url.clone(),
                            version_tag,
                            branch,
                            requested_profile: None,
                            script_action: BuildScriptAction::Upgrade,
                        },
                        package.channel.clone(),
                        &mut build_line_callback,
                    )
                    .await
            };

            match build_result {
                Ok(output) => {
                    let mut install_pkg = package.clone();
                    install_pkg.install_path = None;
                    install_pkg.exec_path = None;
                    install_pkg.build_branch = output.branch.clone();
                    install_pkg.build_commit = output.commit.or(branch_head_commit.clone());
                    install_pkg.version_tag_template = if install_pkg.build_branch.is_some() {
                        None
                    } else {
                        Package::version_tag_template_from_tag(&output.release.tag, &output.version)
                    };
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Phase(PackagePhase::InstallingPackage)
                    );
                    let mut install_message_callback = Some(|line: &str| {
                        let line = line.trim();
                        if !line.is_empty() {
                            progress!(
                                progress_callback,
                                PackageProgressEvent::Warning(line.to_string())
                            );
                            message!(message_callback, "{}", line);
                        }
                    });
                    let install_result = self.installer.install_local_artifact_files(
                        install_pkg,
                        &output.artifact_path,
                        output.version,
                        &mut install_message_callback,
                    );
                    match install_result {
                        Ok(updated) => {
                            rollback_guard.set_partial_package(updated.clone());
                            if cancellation::is_requested() {
                                self.rollback_failed_upgrade(
                                    rollback_guard,
                                    anyhow::anyhow!("Operation interrupted by CTRL-C"),
                                    "Upgrade interrupted",
                                    message_callback,
                                )
                            } else {
                                if let Err(err) = Self::capture_successful_upgrade_rollback(
                                    self.paths,
                                    package,
                                    &backup_path,
                                ) {
                                    let _ = Self::remove_path_if_exists(&backup_path);
                                    rollback_guard.disarm();
                                    Err(err).context(format!(
                                        "Failed to capture rollback for '{}'",
                                        package.name
                                    ))
                                } else {
                                    rollback_guard.disarm();
                                    Ok(updated)
                                }
                            }
                        }
                        Err(err) => self.rollback_failed_upgrade(
                            rollback_guard,
                            err,
                            "Failed to install rebuilt version",
                            message_callback,
                        ),
                    }
                }
                Err(e) => self.rollback_failed_upgrade(
                    rollback_guard,
                    e.context(format!("Failed to rebuild '{}' from source", package.name)),
                    "Failed to rebuild package",
                    message_callback,
                ),
            }
        } else {
            let ResolvedUpgradeTarget::Release(release) = &target else {
                bail!(
                    "Resolved branch target cannot be used for release package '{}'",
                    package.name
                );
            };
            let selected_asset = match self
                .installer
                .resolve_release_asset(package, release, message_callback.as_mut())
                .await
            {
                Ok(asset) => asset,
                Err(err) => {
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Phase(PackagePhase::RollingBack)
                    );
                    return self.rollback_failed_upgrade(
                        rollback_guard,
                        err.context(format!(
                            "Failed to resolve upgrade asset for '{}'",
                            package.name
                        )),
                        "Failed to resolve upgrade asset",
                        message_callback,
                    );
                }
            };

            let install_result = match self
                .try_zsync_upgrade_release(
                    package.clone(),
                    release,
                    &selected_asset,
                    trust_mode,
                    &backup_path,
                    download_progress,
                    message_callback,
                    progress_callback,
                )
                .await
            {
                Ok(Some(updated_package)) => {
                    rollback_guard.set_partial_package(updated_package.clone());
                    Ok(updated_package)
                }
                Ok(None) => {
                    let mut install_pkg = package.clone();
                    install_pkg.install_path = None;
                    install_pkg.exec_path = None;
                    let updated_package = self
                        .installer
                        .install_selected_asset(
                            &self.trusted_keys,
                            install_pkg,
                            release,
                            &selected_asset,
                            &false,
                            trust_mode,
                            download_progress,
                            message_callback,
                            progress_callback,
                        )
                        .await?;
                    rollback_guard.set_partial_package(updated_package.clone());
                    Ok(updated_package)
                }
                Err(err) => {
                    if cancellation::is_requested() {
                        return self.rollback_failed_upgrade(
                            rollback_guard,
                            anyhow::anyhow!("Operation interrupted by CTRL-C"),
                            "Upgrade interrupted",
                            message_callback,
                        );
                    }
                    let summary = output::error_summary(&err);
                    let warning = format!("zsync failed, fallback: {summary}");
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Warning(warning.clone())
                    );
                    message!(message_callback, "{}", warning);
                    let mut install_pkg = package.clone();
                    install_pkg.install_path = None;
                    install_pkg.exec_path = None;
                    let updated_package = self
                        .installer
                        .install_selected_asset(
                            &self.trusted_keys,
                            install_pkg,
                            release,
                            &selected_asset,
                            &false,
                            trust_mode,
                            download_progress,
                            message_callback,
                            progress_callback,
                        )
                        .await?;
                    rollback_guard.set_partial_package(updated_package.clone());
                    Ok(updated_package)
                }
            };

            let mut updated_package = match install_result {
                Ok(updated_package) => updated_package,
                Err(install_err) => {
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Phase(PackagePhase::RollingBack)
                    );

                    return self.rollback_failed_upgrade(
                        rollback_guard,
                        install_err,
                        "Failed to install new version",
                        message_callback,
                    );
                }
            };

            if cancellation::is_requested() {
                return self.rollback_failed_upgrade(
                    rollback_guard,
                    anyhow::anyhow!("Operation interrupted by CTRL-C"),
                    "Upgrade interrupted",
                    message_callback,
                );
            }

            // Restore desktop integration if it existed before
            if had_desktop_integration {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Phase(PackagePhase::CreatingRuntimeLinks)
                );

                if let Err(err) = self
                    .add_desktop_integration(&mut updated_package, message_callback)
                    .await
                {
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Phase(PackagePhase::RollingBack)
                    );
                    return self.rollback_failed_upgrade(
                        rollback_guard,
                        err.context("Failed to restore desktop integration"),
                        "Failed to restore desktop integration",
                        message_callback,
                    );
                }
            }

            if let Err(err) =
                Self::capture_successful_upgrade_rollback(self.paths, package, &backup_path)
            {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Warning(format!(
                        "Warning: failed to capture rollback for '{}': {}",
                        package.name, err
                    ))
                );
                Self::remove_path_if_exists(&backup_path)
                    .context(format!("Failed to remove backup for '{}'", package.name))?;
            }
            rollback_guard.disarm();
            Ok(updated_package)
        }
    }

    async fn add_desktop_integration<H>(
        &self,
        updated_package: &mut Package,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        #[cfg(target_os = "linux")]
        let appimage_extractor = crate::services::artifact::AppImageExtractor::new()
            .context("Failed to initialize appimage extractor")?;

        #[cfg(target_os = "linux")]
        let desktop_manager = DesktopManager::new(self.paths, &appimage_extractor);
        #[cfg(not(target_os = "linux"))]
        let desktop_manager = DesktopManager::new(self.paths);

        desktop_manager
            .enable_package_entry(updated_package, message_callback)
            .await
            .context(format!(
                "Failed to restore desktop entry for '{}'",
                updated_package.name
            ))?;

        Ok(())
    }

    fn rollback_failed_upgrade<H>(
        &self,
        mut rollback: UpgradeRollbackGuard,
        failure: anyhow::Error,
        failure_context: &'static str,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        rollback.disarm();
        let cleanup_result = if let Some(partial) = rollback.partially_installed_package.as_ref() {
            self.remover.remove_package_files(partial, message_callback)
        } else {
            Self::remove_path_if_exists(&rollback.original_install_path)
        };

        if let Err(cleanup_err) = cleanup_result {
            return Err(anyhow::anyhow!(
                "{} for '{}': {}. Rollback failed while removing partial install: {}",
                failure_context,
                rollback.previous_package.name,
                failure,
                cleanup_err
            ));
        }

        fs::rename(&rollback.backup_path, &rollback.original_install_path).context(format!(
            "{} for '{}': {}. Rollback failed while restoring backup",
            failure_context, rollback.previous_package.name, failure
        ))?;

        self.remover
            .restore_runtime_integrations(&rollback.previous_package, message_callback)
            .context(format!(
                "{} for '{}': {}. Rollback failed while restoring runtime links",
                failure_context, rollback.previous_package.name, failure
            ))?;

        Err(failure).context(format!(
            "{} for '{}' (previous version restored)",
            failure_context, rollback.previous_package.name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{PackageUpgrader, ResolvedUpgradeTarget, UpgradeRollbackGuard};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::{
        provider::{Asset, Release},
        upstream::Package,
    };
    use crate::providers::provider_manager::ProviderManager;
    use crate::services::packaging::{PackageInstaller, PackageProgressEvent, PackageRemover};
    use crate::services::trust::TrustedSignatureKeys;
    use crate::utils::{static_paths::UpstreamPaths, test_support};
    use chrono::Utc;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-upgrader-test-{name}-{nanos}"))
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn expected_symlink_path(paths: &UpstreamPaths, name: &str) -> PathBuf {
        let base = paths.state.symlinks_dir.join(name);
        #[cfg(windows)]
        {
            return base.with_extension("exe");
        }
        #[cfg(not(windows))]
        {
            base
        }
    }

    fn test_paths(root: &Path) -> crate::utils::static_paths::UpstreamPaths {
        test_support::upstream_paths(root)
    }

    fn test_package(name: &str, install_path: PathBuf) -> Package {
        let mut package = Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(install_path.clone());
        package.exec_path = Some(install_path);
        package
    }

    fn test_asset(name: &str) -> Asset {
        Asset::new(
            format!("https://example.invalid/{name}"),
            1,
            name.to_string(),
            123,
            Utc::now(),
        )
    }

    #[test]
    fn remove_path_if_exists_handles_files_and_directories() {
        let root = temp_root("remove");
        let file = root.join("f.bin");
        let dir = root.join("d");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(&file, b"content").expect("write file");

        PackageUpgrader::remove_path_if_exists(&file).expect("remove file");
        PackageUpgrader::remove_path_if_exists(&dir).expect("remove dir");
        PackageUpgrader::remove_path_if_exists(&root.join("missing")).expect("ignore missing");

        assert!(!file.exists());
        assert!(!dir.exists());

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn rollback_failed_upgrade_removes_partial_install_and_restores_previous_binary() {
        let root = temp_root("rollback-desktop-failure");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp dir");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks dir");

        let install_path = paths.install.binaries_dir.join("tool");
        let backup_path = paths.install.tmp_dir.join("tool.old");
        fs::write(&install_path, b"new").expect("write partial new binary");
        fs::write(&backup_path, b"old").expect("write backup binary");

        let previous = test_package("tool", install_path.clone());
        let partial = test_package("tool", install_path.clone());
        let provider_manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");
        let installer = PackageInstaller::new(&provider_manager, &paths).expect("installer");
        let remover = PackageRemover::new(&paths);
        let upgrader = PackageUpgrader::new(
            &provider_manager,
            installer,
            remover,
            &paths,
            TrustedSignatureKeys::default(),
        );
        let mut msg = Some(|_: &str| {});

        let err = upgrader
            .rollback_failed_upgrade(
                UpgradeRollbackGuard {
                    previous_package: previous,
                    partially_installed_package: Some(partial),
                    original_install_path: install_path.clone(),
                    backup_path: backup_path.clone(),
                    symlinks_dir: None,
                    armed: true,
                },
                anyhow::anyhow!("desktop failed"),
                "Failed to restore desktop integration",
                &mut msg,
            )
            .expect_err("rollback helper returns original failure");

        assert!(err.to_string().contains("previous version restored"));
        assert_eq!(
            fs::read(&install_path).expect("read restored binary"),
            b"old"
        );
        assert!(!backup_path.exists());
        assert!(expected_symlink_path(&paths, "tool").exists());

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn rollback_failed_upgrade_restores_previous_binary_without_partial_install() {
        let root = temp_root("rollback-install-failure");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp dir");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks dir");

        let install_path = paths.install.binaries_dir.join("tool");
        let backup_path = paths.install.tmp_dir.join("tool.old");
        fs::write(&backup_path, b"old").expect("write backup binary");

        let previous = test_package("tool", install_path.clone());
        let provider_manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");
        let installer = PackageInstaller::new(&provider_manager, &paths).expect("installer");
        let remover = PackageRemover::new(&paths);
        let upgrader = PackageUpgrader::new(
            &provider_manager,
            installer,
            remover,
            &paths,
            TrustedSignatureKeys::default(),
        );
        let mut msg = Some(|_: &str| {});

        let err = upgrader
            .rollback_failed_upgrade(
                UpgradeRollbackGuard {
                    previous_package: previous,
                    partially_installed_package: None,
                    original_install_path: install_path.clone(),
                    backup_path: backup_path.clone(),
                    symlinks_dir: None,
                    armed: true,
                },
                anyhow::anyhow!("already installed"),
                "Failed to install new version",
                &mut msg,
            )
            .expect_err("rollback helper returns original failure");

        assert!(err.to_string().contains("previous version restored"));
        assert_eq!(
            fs::read(&install_path).expect("read restored binary"),
            b"old"
        );
        assert!(!backup_path.exists());
        assert!(expected_symlink_path(&paths, "tool").exists());

        cleanup(&root).expect("cleanup");
    }

    #[tokio::test]
    async fn upgrade_resolved_rolls_back_when_asset_selection_fails() {
        let root = temp_root("resolve-asset-failure");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp dir");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks dir");

        let install_path = paths.install.binaries_dir.join("tool");
        fs::write(&install_path, b"old").expect("write installed binary");

        let mut package = test_package("tool", install_path.clone());
        package.version = crate::models::common::Version::new(3, 11, 15, false);

        let release = Release {
            id: 1,
            tag: "v3.12.0".to_string(),
            name: "v3.12.0".to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets: vec![Asset::new(
                "https://example.invalid/tool.AppImage".to_string(),
                2,
                "tool.AppImage".to_string(),
                123,
                Utc::now(),
            )],
            version: crate::models::common::Version::new(3, 12, 0, false),
            published_at: Utc::now(),
        };

        let provider_manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");
        let installer = PackageInstaller::new(&provider_manager, &paths).expect("installer");
        let remover = PackageRemover::new(&paths);
        let upgrader = PackageUpgrader::new(
            &provider_manager,
            installer,
            remover,
            &paths,
            TrustedSignatureKeys::default(),
        );
        let mut download = Some(|_: u64, _: u64| {});
        let mut msg = Some(|_: &str| {});
        let mut progress = Some(|_: PackageProgressEvent| {});

        let err = upgrader
            .upgrade_resolved(
                &package,
                ResolvedUpgradeTarget::Release(release),
                crate::models::common::enums::TrustMode::None,
                &mut download,
                &mut msg,
                &mut progress,
            )
            .await
            .expect_err("upgrade should fail and roll back");

        assert!(
            err.to_string()
                .contains("Failed to resolve upgrade asset for 'tool'")
        );
        assert_eq!(fs::read(&install_path).expect("restored install"), b"old");
        assert!(expected_symlink_path(&paths, "tool").exists());
        assert!(!paths.install.tmp_dir.join("tool.old").exists());

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn can_apply_zsync_accepts_direct_file_binary_upgrades() {
        let root = temp_root("zsync-binary");
        let install_path = root.join("tool");
        fs::create_dir_all(&root).expect("create root");
        fs::write(&install_path, b"old").expect("write installed file");

        let package = test_package("tool", install_path.clone());
        let asset = test_asset("tool");

        assert!(PackageUpgrader::can_apply_zsync(
            &package,
            &asset,
            &install_path
        ));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn can_apply_zsync_rejects_archives_and_directories() {
        let root = temp_root("zsync-archive");
        let install_dir = root.join("tool");
        fs::create_dir_all(&install_dir).expect("create installed dir");

        let mut package = test_package("tool", install_dir.clone());
        package.filetype = Filetype::Archive;
        let asset = test_asset("tool.tar.gz");

        assert!(!PackageUpgrader::can_apply_zsync(
            &package,
            &asset,
            &install_dir
        ));

        cleanup(&root).expect("cleanup");
    }
}
