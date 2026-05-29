#[cfg(target_os = "linux")]
use crate::services::integration::AppImageExtractor;
use crate::{
    models::{
        common::{
            DesktopEntry,
            enums::{Channel, TrustMode},
        },
        provider::Release,
        upstream::{InstallType, Package},
    },
    providers::provider_manager::ProviderManager,
    services::builder::{BuildRequest, worker::BuildWorker},
    services::{
        integration::{DesktopManager, IconManager},
        packaging::RollbackManager,
        packaging::{PackageInstaller, PackagePhase, PackageProgressEvent, PackageRemover},
        storage::rollback_storage::{RollbackRecord, RollbackSource, RollbackStorage},
        trust::TrustedSignatureKeys,
    },
    utils::static_paths::UpstreamPaths,
};

use anyhow::{Context, Result, bail};
use console::style;
use std::{
    fs,
    path::{Path, PathBuf},
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

impl<'a> PackageUpgrader<'a> {
    fn backup_path(install_path: &Path) -> Result<PathBuf> {
        let file_name = install_path.file_name().ok_or_else(|| {
            anyhow::anyhow!("Install path '{}' has no filename", install_path.display())
        })?;
        Ok(install_path.with_file_name(format!("{}.old", file_name.to_string_lossy())))
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

    fn path_relative_to(base: &Path, full: &Path) -> Result<PathBuf> {
        full.strip_prefix(base).map(Path::to_path_buf).map_err(|_| {
            anyhow::anyhow!(
                "Path '{}' is not under '{}'",
                full.display(),
                base.display()
            )
        })
    }

    fn capture_successful_upgrade_rollback(
        paths: &UpstreamPaths,
        package: &Package,
        backup_path: &Path,
    ) -> Result<()> {
        let rollback_file = RollbackManager::rollback_file_path(paths);
        let mut rollback_storage = RollbackStorage::new(&rollback_file)?;
        let package_rollback_dir = paths.install.rollback_dir.join(&package.name);
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

        let backup_name = backup_path.file_name().ok_or_else(|| {
            anyhow::anyhow!("Backup path '{}' has no file name", backup_path.display())
        })?;
        let rollback_artifact = package_rollback_dir.join(backup_name);
        crate::utils::filesystem::safe_move::move_file_or_dir(backup_path, &rollback_artifact)?;

        let icon_relative_path = if let Some(icon_path) = package.icon_path.as_ref() {
            if icon_path.exists() {
                let icon_name = icon_path.file_name().ok_or_else(|| {
                    anyhow::anyhow!("Icon path '{}' has no file name", icon_path.display())
                })?;
                let icon_backup =
                    package_rollback_dir.join(format!("icon-{}", icon_name.to_string_lossy()));
                fs::copy(icon_path, &icon_backup).context(format!(
                    "Failed to copy icon '{}' to '{}'",
                    icon_path.display(),
                    icon_backup.display()
                ))?;
                Some(Self::path_relative_to(
                    &paths.install.rollback_dir,
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
            artifact_relative_path: Self::path_relative_to(
                &paths.install.rollback_dir,
                &rollback_artifact,
            )?,
            icon_relative_path,
            source: RollbackSource::Upgrade,
            created_at: chrono::Utc::now(),
        };
        rollback_storage.upsert_record(&package.name, record)?;
        Ok(())
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
                    .context(format!(
                        "Failed to fetch latest release for '{}'",
                        package.name
                    ))?
            } else {
                let Some(latest_release) = self
                    .provider_manager
                    .check_for_updates(package)
                    .await
                    .context(format!(
                    "Failed to fetch latest release for '{}'",
                    package.name
                ))?
                else {
                    message!(message_callback, "'{}' is already up to date", package.name);
                    return Ok(None);
                };

                latest_release
            };

            if !force {
                let up_to_date = if package.channel == Channel::Nightly {
                    release.published_at <= package.last_upgraded
                } else {
                    !release.version.is_newer_than(&package.version)
                };

                if up_to_date {
                    message!(message_callback, "'{}' is already up to date", package.name);
                    return Ok(None);
                }
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
                )
                .await
                .map(Some);
        }

        let Some(target) = target else {
            return Ok(None);
        };

        self.upgrade_resolved(
            package,
            target,
            trust_mode,
            download_progress,
            message_callback,
        )
        .await
        .map(Some)
    }

    pub async fn upgrade_resolved<F, H>(
        &self,
        package: &Package,
        target: ResolvedUpgradeTarget,
        trust_mode: TrustMode,
        download_progress: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let mut no_progress: Option<fn(PackageProgressEvent)> = None;
        self.upgrade_resolved_with_progress(
            package,
            target,
            trust_mode,
            download_progress,
            message_callback,
            &mut no_progress,
        )
        .await
    }

    pub async fn upgrade_resolved_with_progress<F, H, P>(
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
        let backup_path = Self::backup_path(&original_install_path)?;

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
            let worker = BuildWorker::new(self.provider_manager);
            let mut build_line_callback = Some(|line: &str| {
                let line = line.trim();
                if !line.is_empty() {
                    progress!(
                        progress_callback,
                        PackageProgressEvent::Warning(line.to_string())
                    );
                }
            });
            let build_result = worker
                .build(
                    BuildRequest {
                        name: package.name.clone(),
                        repo_slug: package.repo_slug.clone(),
                        provider: package.provider.clone(),
                        base_url: package.base_url.clone(),
                        version_tag,
                        branch,
                        requested_profile: None,
                        build_output: None,
                    },
                    package.channel.clone(),
                    &mut build_line_callback,
                )
                .await;
            drop(build_line_callback);

            match build_result {
                Ok(output) => {
                    let mut install_pkg = package.clone();
                    install_pkg.build_branch = output.branch.clone();
                    install_pkg.build_commit = output.commit.or(branch_head_commit.clone());
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
                    self.installer.install_local_artifact(
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
            self.installer
                .install_package_files(
                    package.clone(),
                    release,
                    trust_mode,
                    &self.trusted_keys,
                    download_progress,
                    message_callback,
                    progress_callback,
                )
                .await
        };
        let mut updated_package = match install_result {
            Ok(updated_package) => updated_package,
            Err(install_err) => {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Phase(PackagePhase::RollingBack)
                );

                let _ = Self::remove_path_if_exists(&original_install_path);
                progress!(
                    progress_callback,
                    PackageProgressEvent::Phase(PackagePhase::RestoringSnapshot)
                );
                fs::rename(&backup_path, &original_install_path).context(format!(
                    "Upgrade failed for '{}': {}. Rollback failed while restoring backup",
                    package.name, install_err
                ))?;

                self.remover
                    .restore_runtime_integrations(package, message_callback)
                    .context(format!(
                        "Upgrade failed for '{}': {}. Rollback failed while restoring runtime links",
                        package.name, install_err
                    ))?;

                return Err(install_err).context(format!(
                    "Failed to install new version of '{}' (previous version restored)",
                    package.name
                ));
            }
        };

        // Restore desktop integration if it existed before
        if had_desktop_integration {
            progress!(
                progress_callback,
                PackageProgressEvent::Phase(PackagePhase::CreatingRuntimeLinks)
            );

            #[cfg(target_os = "linux")]
            let appimage_extractor =
                AppImageExtractor::new().context("Failed to initialize appimage extractor")?;

            #[cfg(target_os = "linux")]
            let icon_manager = IconManager::new(self.paths, &appimage_extractor);
            #[cfg(not(target_os = "linux"))]
            let icon_manager = IconManager::new(self.paths);

            #[cfg(target_os = "linux")]
            let desktop_manager = DesktopManager::new(self.paths, &appimage_extractor);
            #[cfg(not(target_os = "linux"))]
            let desktop_manager = DesktopManager::new(self.paths);
            let install_path = updated_package.install_path.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "Package '{}' has no install path after upgrade",
                    updated_package.name
                )
            })?;

            let icon_path = icon_manager
                .add_icon(
                    &updated_package.name,
                    &install_path,
                    &updated_package.filetype,
                    message_callback,
                )
                .await
                .context(format!("Failed to add icon for '{}'", updated_package.name))?;

            updated_package.icon_path = icon_path;

            let desktop_entry = DesktopEntry::from_package(&updated_package);

            let _ = desktop_manager
                .create_entry(
                    &install_path,
                    &updated_package.filetype,
                    desktop_entry,
                    message_callback,
                )
                .await
                .context(format!(
                    "Failed to create desktop entry for '{}'",
                    updated_package.name
                ))?;
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
}

#[cfg(test)]
mod tests {
    use super::PackageUpgrader;
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

    #[test]
    fn backup_path_appends_old_suffix() {
        let original = Path::new("/tmp/example/tool");
        let backup = PackageUpgrader::backup_path(original).expect("backup path");
        assert!(backup.ends_with("tool.old"));
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
}
