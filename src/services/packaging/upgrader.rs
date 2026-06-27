use crate::{
    models::{
        common::enums::TrustMode,
        provider::{Asset, Release},
        upstream::{InstallType, Package},
    },
    providers::provider_manager::ProviderManager,
    routines::builder::{BuildRequest, scripts::BuildScriptAction, worker::BuildWorker},
    services::{
        artifact::zsync_handler,
        integration::{CompletionManager, DesktopManager},
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

struct FailedUpgradeRollback<'a> {
    previous_package: &'a Package,
    partially_installed_package: Option<&'a Package>,
    original_install_path: &'a Path,
    backup_path: &'a Path,
    failure_context: &'a str,
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

        let work_target = work_root.join(backup_path.file_name().ok_or_else(|| {
            anyhow::anyhow!("Backup path '{}' has no filename", backup_path.display())
        })?);
        fs::copy(backup_path, &work_target).context(format!(
            "Failed to stage '{}' for zsync update",
            backup_path.display()
        ))?;

        let result: Result<Package> = async {
            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::DownloadingPackage)
            );
            zsync_handler::update_selected_asset(
                &package,
                release,
                asset,
                self.provider_manager,
                &work_cache,
                &work_target,
                message_callback.as_mut(),
            )
            .await
            .context(format!("Failed to update '{}' via zsync", package.name))?;

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
        Ok(Some(result?))
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
        let install_result = if package.install_type == InstallType::Build {
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
                    self.installer.install_local_artifact_files(
                        install_pkg,
                        &output.artifact_path,
                        output.version,
                        &mut install_message_callback,
                    )
                }
                Err(e) => {
                    Err(e).context(format!("Failed to rebuild '{}' from source", package.name))
                }
            }
        } else {
            let ResolvedUpgradeTarget::Release(release) = &target else {
                bail!(
                    "Resolved branch target cannot be used for release package '{}'",
                    package.name
                );
            };
            let selected_asset = self
                .installer
                .resolve_release_asset(&package, release, message_callback.as_mut())
                .await
                .context(format!(
                    "Failed to resolve upgrade asset for '{}'",
                    package.name
                ))?;

            match self
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
                Ok(Some(updated_package)) => Ok(updated_package),
                Ok(None) => {
                    let mut install_pkg = package.clone();
                    install_pkg.install_path = None;
                    install_pkg.exec_path = None;
                    self.installer
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
                        .await
                }
                Err(err) => {
                    let warning = format!(
                        "zsync update failed for '{}'; falling back to direct download: {}",
                        package.name, err
                    );
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Warning(warning.clone())
                    );
                    message!(message_callback, "{}", warning);
                    let mut install_pkg = package.clone();
                    install_pkg.install_path = None;
                    install_pkg.exec_path = None;
                    self.installer
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
                        .await
                }
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
                    FailedUpgradeRollback {
                        previous_package: package,
                        partially_installed_package: None,
                        original_install_path: &original_install_path,
                        backup_path: &backup_path,
                        failure_context: "Failed to install new version",
                    },
                    install_err,
                    message_callback,
                );
            }
        };

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
                    FailedUpgradeRollback {
                        previous_package: package,
                        partially_installed_package: Some(&updated_package),
                        original_install_path: &original_install_path,
                        backup_path: &backup_path,
                        failure_context: "Failed to restore desktop integration",
                    },
                    err.context("Failed to restore desktop integration"),
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

        Ok(updated_package)
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
        rollback: FailedUpgradeRollback<'_>,
        failure: anyhow::Error,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let cleanup_result = if let Some(partial) = rollback.partially_installed_package {
            self.remover.remove_package_files(partial, message_callback)
        } else {
            Self::remove_path_if_exists(rollback.original_install_path)
        };

        if let Err(cleanup_err) = cleanup_result {
            return Err(anyhow::anyhow!(
                "{} for '{}': {}. Rollback failed while removing partial install: {}",
                rollback.failure_context,
                rollback.previous_package.name,
                failure,
                cleanup_err
            ));
        }

        fs::rename(rollback.backup_path, rollback.original_install_path).context(format!(
            "{} for '{}': {}. Rollback failed while restoring backup",
            rollback.failure_context, rollback.previous_package.name, failure
        ))?;

        self.remover
            .restore_runtime_integrations(rollback.previous_package, message_callback)
            .context(format!(
                "{} for '{}': {}. Rollback failed while restoring runtime links",
                rollback.failure_context, rollback.previous_package.name, failure
            ))?;

        Err(failure).context(format!(
            "{} for '{}' (previous version restored)",
            rollback.failure_context, rollback.previous_package.name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{FailedUpgradeRollback, PackageUpgrader};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::{provider::Asset, upstream::Package};
    use crate::providers::provider_manager::ProviderManager;
    use crate::services::packaging::{PackageInstaller, PackageRemover};
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
        let base = paths.integration.symlinks_dir.join(name);
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
        fs::create_dir_all(&paths.integration.symlinks_dir).expect("create symlinks dir");

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
                FailedUpgradeRollback {
                    previous_package: &previous,
                    partially_installed_package: Some(&partial),
                    original_install_path: &install_path,
                    backup_path: &backup_path,
                    failure_context: "Failed to restore desktop integration",
                },
                anyhow::anyhow!("desktop failed"),
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
        fs::create_dir_all(&paths.integration.symlinks_dir).expect("create symlinks dir");

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
                FailedUpgradeRollback {
                    previous_package: &previous,
                    partially_installed_package: None,
                    original_install_path: &install_path,
                    backup_path: &backup_path,
                    failure_context: "Failed to install new version",
                },
                anyhow::anyhow!("already installed"),
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

    #[test]
    fn zsync_work_root_uses_upstream_temp_dir() {
        let root = temp_root("zsync-work-root");
        let paths = test_paths(&root);
        let package = test_package("tool", root.join("tool"));

        let work_root = PackageUpgrader::zsync_work_root(&paths, &package);

        assert_eq!(
            work_root.parent().expect("work root parent"),
            paths.install.tmp_dir
        );

        cleanup(&root).expect("cleanup");
    }
}
