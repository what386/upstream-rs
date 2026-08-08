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
        packaging::{
            PackageInstaller, PackagePhase, PackageProgressEvent, PackageReplacer,
            installer::InstallWorkspace, replacement::PreparedInstall,
        },
        trust::{TrustVerifier, TrustedSignatureKeys},
    },
    storage::rollback::RollbackSource,
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, bail};
use console::style;
use std::path::Path;

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
    paths: &'a UpstreamPaths,
    trusted_keys: TrustedSignatureKeys,
}

#[derive(Clone)]
pub enum ResolvedUpgradeTarget {
    Release(Release),
    Branch { branch: String, head_commit: String },
}

impl<'a> PackageUpgrader<'a> {
    #[cfg(test)]
    fn can_apply_zsync(package: &Package, asset: &Asset, install_path: &Path) -> bool {
        zsync_handler::can_update_package(package, asset, install_path)
    }

    #[allow(clippy::too_many_arguments)]
    async fn try_zsync_upgrade_release<F, H, P>(
        &self,
        installer: &PackageInstaller<'_>,
        package: Package,
        release: &Release,
        asset: &Asset,
        trust_mode: TrustMode,
        source_path: &Path,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Option<Package>>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        if let Some(callback) = progress_callback.as_mut() {
            callback(PackageProgressEvent::Phase(
                PackagePhase::ApplyingZsyncUpdate,
            ));
        }
        let progress_cell = std::cell::RefCell::new(progress_callback.as_mut());
        let mut zsync_progress = Some(|downloaded: u64, total: u64| {
            if let Some(callback) = progress_cell.borrow_mut().as_deref_mut() {
                callback(PackageProgressEvent::Zsync { downloaded, total });
            }
        });
        let Some(updated) = zsync_handler::update_package_asset(
            &package,
            release,
            asset,
            self.provider_manager,
            &self.paths.install.tmp_dir,
            source_path,
            message_callback,
            &mut zsync_progress,
        )
        .await?
        else {
            return Ok(None);
        };
        let verifier = TrustVerifier::new(
            self.provider_manager,
            updated.cache(),
            trust_mode,
            &self.trusted_keys,
        );
        verifier
            .verify_file(
                updated.path(),
                release,
                &package.provider,
                &mut None::<fn(u64, u64)>,
                message_callback,
                progress_callback,
            )
            .await
            .context("Failed trust verification for zsync-updated artifact")?;
        if let Some(callback) = progress_callback.as_mut() {
            callback(PackageProgressEvent::Phase(PackagePhase::InstallingPackage));
        }
        let mut staged = package;
        staged.install_path = None;
        staged.exec_path = None;
        staged.icon_path = None;
        staged.version = release.version.clone();
        staged.record_release(release);
        let installed = installer
            .install_local_artifact_files(
                staged,
                updated.path(),
                release.version.clone(),
                message_callback,
            )
            .context("Failed to install zsync-updated artifact")?;
        let name = installed.name.clone();
        let provider = installed.provider.clone();
        let result = installer
            .finish_verified_release_install(
                installed,
                &name,
                &provider,
                release,
                updated.cache(),
                message_callback,
                progress_callback,
            )
            .await
            .map(Some);
        if let Some(callback) = download_progress.as_mut() {
            callback(0, 0);
        }
        result
    }

    pub fn new(
        provider_manager: &'a ProviderManager,
        installer: PackageInstaller<'a>,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
    ) -> Self {
        Self {
            provider_manager,
            installer,
            paths,
            trusted_keys,
        }
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
        self.replace_resolved(
            package,
            target,
            trust_mode,
            false,
            RollbackSource::Upgrade,
            "Upgrading",
            download_progress,
            message_callback,
            progress_callback,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reinstall_resolved<F, H, P>(
        &self,
        package: &Package,
        target: ResolvedUpgradeTarget,
        trust_mode: TrustMode,
        force: bool,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        self.replace_resolved(
            package,
            target,
            trust_mode,
            force,
            RollbackSource::Reinstall,
            "Reinstalling",
            download_progress,
            message_callback,
            progress_callback,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn replace_resolved<F, H, P>(
        &self,
        package: &Package,
        target: ResolvedUpgradeTarget,
        trust_mode: TrustMode,
        allow_pinned: bool,
        rollback_source: RollbackSource,
        action: &'static str,
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
        if package.is_pinned && !allow_pinned {
            bail!("Package '{}' is pinned", package.name);
        }

        let had_desktop_integration = package.icon_path.is_some();

        message!(
            message_callback,
            "{}",
            style(format!("{action} '{}' ...", package.name)).cyan()
        );

        let original_install_path = package
            .install_path
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!("Package '{}' has no install path recorded", package.name)
            })?
            .clone();
        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::CreatingSnapshot)
        );
        let workspace = InstallWorkspace::new(self.paths, &package.name)?;
        let mut staging_installer =
            PackageInstaller::new_for_workspace(self.provider_manager, self.paths, workspace)?;

        // Prepare the complete replacement without touching the active package.

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
                            PackageProgressEvent::Detail(line.to_string())
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
                    install_pkg.icon_path = None;
                    install_pkg.build_branch = output.branch.clone();
                    install_pkg.build_commit = output.commit.or(branch_head_commit.clone());
                    if install_pkg.build_branch.is_some() {
                        install_pkg.release_tag = None;
                        install_pkg.release_published_at = None;
                        install_pkg.version_tag_template = None;
                    } else {
                        install_pkg.record_release(&output.release);
                    }
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Phase(PackagePhase::InstallingPackage)
                    );
                    let mut install_message_callback = Some(|line: &str| {
                        let line = line.trim();
                        if !line.is_empty() {
                            progress!(
                                progress_callback,
                                PackageProgressEvent::Detail(line.to_string())
                            );
                            message!(message_callback, "{}", line);
                        }
                    });
                    let install_result = staging_installer.install_local_artifact_files(
                        install_pkg,
                        &output.artifact_path,
                        output.version,
                        &mut install_message_callback,
                    );
                    match install_result {
                        Ok(mut updated_package) => {
                            updated_package.icon_path = package.icon_path.clone();
                            let workspace = staging_installer.take_workspace()?;
                            PackageReplacer::new(self.paths)
                                .replace(
                                    package,
                                    PreparedInstall::new(updated_package, workspace),
                                    false,
                                    rollback_source.clone(),
                                    message_callback,
                                    progress_callback,
                                )
                                .await
                        }
                        Err(err) => Err(err).context("Failed to prepare rebuilt version"),
                    }
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
                    return Err(err).context(format!(
                        "Failed to resolve upgrade asset for '{}'",
                        package.name
                    ));
                }
            };

            let install_result = match self
                .try_zsync_upgrade_release(
                    &staging_installer,
                    package.clone(),
                    release,
                    &selected_asset,
                    trust_mode,
                    &original_install_path,
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
                    install_pkg.icon_path = None;
                    let result = staging_installer
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
                        .await;
                    result
                }
                Err(err) => {
                    if cancellation::is_requested() {
                        return Err(anyhow::anyhow!("Operation interrupted by CTRL-C"));
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
                    install_pkg.icon_path = None;
                    let result = staging_installer
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
                        .await;
                    result
                }
            };

            let updated_package = match install_result {
                Ok(updated_package) => updated_package,
                Err(install_err) => {
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Phase(PackagePhase::RollingBack)
                    );

                    return Err(install_err).context("Failed to prepare new version");
                }
            };

            let workspace = staging_installer.take_workspace()?;
            PackageReplacer::new(self.paths)
                .replace(
                    package,
                    PreparedInstall::new(updated_package, workspace),
                    had_desktop_integration,
                    rollback_source,
                    message_callback,
                    progress_callback,
                )
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PackageUpgrader, ResolvedUpgradeTarget};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::{
        provider::{Asset, Release},
        upstream::Package,
    };
    use crate::providers::provider_manager::ProviderManager;
    use crate::services::integration::SymlinkManager;
    use crate::services::packaging::{
        PackageInstaller, PackageProgressEvent, RollbackManager,
        replacement::{PackageReplacer, ReplacementBackup},
    };
    use crate::services::trust::TrustedSignatureKeys;
    use crate::storage::rollback::RollbackStorage;
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

        PackageReplacer::remove_path_if_exists(&file).expect("remove file");
        PackageReplacer::remove_path_if_exists(&dir).expect("remove dir");
        PackageReplacer::remove_path_if_exists(&root.join("missing")).expect("ignore missing");

        assert!(!file.exists());
        assert!(!dir.exists());

        cleanup(&root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_failed_upgrade_removes_partial_install_and_restores_previous_binary() {
        let root = temp_root("rollback-desktop-failure");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp dir");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks dir");
        fs::create_dir_all(&paths.integration.bash_completions_dir)
            .expect("create bash completions dir");
        fs::create_dir_all(&paths.integration.xdg_applications_dir)
            .expect("create applications dir");
        fs::create_dir_all(&paths.state.icons_dir).expect("create icons dir");

        let install_path = paths.install.binaries_dir.join("tool");
        let backup_dir = paths.install.tmp_dir.join("tool.old");
        let backup_path = backup_dir.join("package/tool");
        fs::write(&install_path, b"new").expect("write partial new binary");
        fs::create_dir_all(backup_path.parent().expect("backup parent"))
            .expect("create backup directory");
        fs::write(&backup_path, b"old").expect("write backup binary");

        let desktop_path = paths.integration.xdg_applications_dir.join("tool.desktop");
        let old_icon_path = paths.state.icons_dir.join("tool-old.png");
        let new_icon_path = paths.state.icons_dir.join("tool-new.png");
        fs::write(&desktop_path, b"old desktop").expect("write old desktop");
        fs::write(&old_icon_path, b"old icon").expect("write old icon");
        let mut previous = test_package("tool", install_path.clone());
        previous.icon_path = Some(old_icon_path.clone());
        let mut partial = test_package("tool", install_path.clone());
        partial.icon_path = Some(new_icon_path.clone());
        let completion_path = paths.integration.bash_completions_dir.join("tool");
        fs::write(&completion_path, b"old completion").expect("write old completion");
        let mut rollback_guard =
            ReplacementBackup::new(previous, install_path.clone(), backup_dir.clone())
                .expect("create replacement backup");
        rollback_guard
            .move_integrations(&paths)
            .expect("move integrations");
        rollback_guard.set_partial_package(partial);
        fs::write(&completion_path, b"new completion").expect("write new completion");
        fs::write(&desktop_path, b"new desktop").expect("write new desktop");
        fs::write(&new_icon_path, b"new icon").expect("write new icon");
        let mut msg = Some(|_: &str| {});

        let result: anyhow::Result<()> = PackageReplacer::new(&paths).restore_after_failure(
            rollback_guard,
            anyhow::anyhow!("desktop failed"),
            "Failed to restore desktop integration",
            &mut msg,
        );
        let err = result.expect_err("rollback helper returns original failure");

        assert!(err.to_string().contains("previous version restored"));
        assert_eq!(
            fs::read(&install_path).expect("read restored binary"),
            b"old"
        );
        assert!(!backup_dir.exists());
        assert!(expected_symlink_path(&paths, "tool").exists());
        assert_eq!(
            fs::read(&completion_path).expect("read restored completion"),
            b"old completion"
        );
        assert_eq!(
            fs::read(&desktop_path).expect("read restored desktop"),
            b"old desktop"
        );
        assert_eq!(
            fs::read(&old_icon_path).expect("read restored icon"),
            b"old icon"
        );
        assert!(!new_icon_path.exists());

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
        let backup_dir = paths.install.tmp_dir.join("tool.old");
        let backup_path = backup_dir.join("package/tool");
        fs::create_dir_all(backup_path.parent().expect("backup parent"))
            .expect("create backup directory");
        fs::write(&backup_path, b"old").expect("write backup binary");

        let previous = test_package("tool", install_path.clone());
        let mut msg = Some(|_: &str| {});

        let result: anyhow::Result<()> = PackageReplacer::new(&paths).restore_after_failure(
            ReplacementBackup::new(previous, install_path.clone(), backup_dir.clone())
                .expect("create replacement backup"),
            anyhow::anyhow!("already installed"),
            "Failed to install new version",
            &mut msg,
        );
        let err = result.expect_err("rollback helper returns original failure");

        assert!(err.to_string().contains("previous version restored"));
        assert_eq!(
            fs::read(&install_path).expect("read restored binary"),
            b"old"
        );
        assert!(!backup_dir.exists());
        assert!(expected_symlink_path(&paths, "tool").exists());

        cleanup(&root).expect("cleanup");
    }

    #[tokio::test]
    async fn rollback_capture_failure_restores_previous_install() {
        let root = temp_root("capture-failure");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp dir");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks dir");
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config dir");
        fs::write(
            &paths.config.config_file,
            "[rollback]\ncompression_level = \"low\"\nstored_artifacts = 1\n",
        )
        .expect("write rollback config");

        let install_path = paths.install.binaries_dir.join("tool");
        let backup_dir = paths.install.tmp_dir.join("tool.old");
        let backup_path = backup_dir.join("package/tool");
        fs::write(&install_path, b"new").expect("write replacement");
        fs::create_dir_all(backup_path.parent().expect("backup parent"))
            .expect("create backup directory");
        fs::write(&backup_path, b"old").expect("write backup");
        let guard_previous = test_package("tool", install_path.clone());
        let mut previous = guard_previous.clone();
        let invalid_icon = root.join("icon-directory");
        fs::create_dir_all(&invalid_icon).expect("create invalid icon directory");
        previous.icon_path = Some(invalid_icon);
        let updated = test_package("tool", install_path.clone());
        let mut guard =
            ReplacementBackup::new(guard_previous, install_path.clone(), backup_dir.clone())
                .expect("create replacement backup");
        guard.set_partial_package(updated.clone());

        let capture_error = PackageReplacer::capture_rollback(
            &paths,
            &previous,
            &backup_path,
            previous.icon_path.as_deref(),
            crate::storage::rollback::RollbackSource::Upgrade,
        )
        .expect_err("post-copy icon failure should prevent rollback capture");
        let result: anyhow::Result<()> = PackageReplacer::new(&paths).restore_after_failure(
            guard,
            capture_error,
            "Failed to finalize replacement",
            &mut None::<fn(&str)>,
        );
        let error = result.expect_err("post-copy icon failure should prevent rollback capture");

        assert!(error.to_string().contains("previous version restored"));
        assert_eq!(fs::read(&install_path).expect("read restored"), b"old");
        assert!(!backup_dir.exists());

        cleanup(&root).expect("cleanup");
    }

    #[tokio::test]
    async fn successful_replacement_removes_transient_old_backup() {
        let root = temp_root("capture-success");
        let paths = test_paths(&root);
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries dir");
        fs::create_dir_all(&paths.install.tmp_dir).expect("create tmp dir");
        fs::create_dir_all(&paths.state.symlinks_dir).expect("create symlinks dir");
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config dir");
        fs::write(
            &paths.config.config_file,
            "[rollback]\ncompression_level = \"low\"\nstored_artifacts = 1\n",
        )
        .expect("write rollback config");

        let install_path = paths.install.binaries_dir.join("tool");
        let backup_path = paths.install.tmp_dir.join("tool-backup.old");
        fs::write(&install_path, b"new").expect("write replacement");
        fs::write(&backup_path, b"old").expect("write backup");
        let previous = test_package("tool", install_path.clone());
        PackageReplacer::capture_rollback(
            &paths,
            &previous,
            &backup_path,
            None,
            crate::storage::rollback::RollbackSource::Upgrade,
        )
        .expect("capture rollback");
        PackageReplacer::remove_path_if_exists(&backup_path).expect("remove transient backup");

        assert_eq!(fs::read(&install_path).expect("read replacement"), b"new");
        assert!(!backup_path.exists());
        let rollback_file = RollbackManager::rollback_file_path(&paths);
        let rollback_storage =
            RollbackStorage::new(&rollback_file).expect("reload rollback storage");
        assert!(rollback_storage.get_record("tool").is_some());

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
        SymlinkManager::new(&paths.state.symlinks_dir)
            .add_link(&install_path, "tool")
            .expect("create initial runtime link");

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
        let upgrader = PackageUpgrader::new(
            &provider_manager,
            installer,
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
        assert!(
            fs::read_dir(&paths.install.tmp_dir)
                .expect("read tmp")
                .filter_map(Result::ok)
                .all(|entry| entry.path().extension().and_then(|ext| ext.to_str()) != Some("old"))
        );

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
