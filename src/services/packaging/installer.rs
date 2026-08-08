mod staging;

use crate::{
    models::common::enums::TrustMode,
    models::{
        common::{Version, enums::Filetype},
        provider::{Asset, Release},
        upstream::Package,
    },
    providers::provider_manager::ProviderManager,
    services::{
        artifact::{archive_layout, compression_handler, dotslash_parser, permission_handler},
        integration::CompletionManager,
        packaging::{
            InstallPlan, InstallRequest, InstallSource, PackagePhase, PackageProgressEvent,
            PlannedInstallSource,
            disk_impact::{DiskImpact, asset_size_estimate, install_impact_from_download},
            replacement::{PackageReplacer, PreparedInstall},
        },
        trust::{
            ChecksumVerificationStatus, SignatureScheme, SignatureVerificationStatus,
            TrustVerificationStatus, TrustVerifier, TrustedSignatureKeys,
        },
    },
    utils::{filesystem::safe_move, static_paths::UpstreamPaths},
};
pub(crate) use staging::InstallWorkspace;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use console::style;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::storage::database::PackageDatabase;

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

pub struct PackageInstaller<'a> {
    provider_manager: &'a ProviderManager,
    paths: &'a UpstreamPaths,
    workspace: Option<InstallWorkspace>,
    download_cache: PathBuf,
    extract_cache: PathBuf,
}

pub struct ResolvedAssetInstall {
    pub release_name: String,
    pub release_tag: String,
    pub asset_name: String,
    pub resolved_filetype: Filetype,
    pub disk_impact: DiskImpact,
    pub release: Release,
    pub asset: Asset,
}

impl<'a> PackageInstaller<'a> {
    pub(crate) fn paths(&self) -> &UpstreamPaths {
        self.paths
    }

    pub(crate) fn provider_manager(&self) -> &ProviderManager {
        self.provider_manager
    }

    pub async fn plan_install(&self, request: InstallRequest) -> Result<InstallPlan> {
        if request.package.install_path.is_some() {
            return Err(anyhow!(
                "Package '{}' is already installed",
                request.package.name
            ));
        }
        let source = match request.source {
            InstallSource::Release { version, semver } => {
                let resolved = self
                    .preview_single_install(&request.package, &version, &semver)
                    .await?;
                PlannedInstallSource::Release {
                    release: resolved.release,
                    asset: resolved.asset,
                }
            }
            InstallSource::SelectedRelease { release, asset } => {
                PlannedInstallSource::Release { release, asset }
            }
            InstallSource::LocalArtifact {
                artifact_path,
                version,
            } => {
                if !artifact_path.exists() {
                    return Err(anyhow!(
                        "Local artifact path '{}' does not exist",
                        artifact_path.display()
                    ));
                }
                PlannedInstallSource::LocalArtifact {
                    artifact_path,
                    version,
                }
            }
        };
        let disk_impact = match &source {
            PlannedInstallSource::Release { asset, .. } => {
                install_impact_from_download(asset_size_estimate(asset.size))
            }
            PlannedInstallSource::LocalArtifact { .. } => DiskImpact::unknown(),
        };
        Ok(InstallPlan {
            package: request.package,
            source,
            add_entry: request.add_entry,
            trust_mode: request.trust_mode,
            disk_impact,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn install<F, H, P>(
        &mut self,
        package_database: &mut PackageDatabase,
        trusted_keys: &TrustedSignatureKeys,
        plan: InstallPlan,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        self.ensure_name_available(package_database, &plan.package.name)?;
        let add_entry = plan.add_entry;
        let installed = self
            .materialize_install(
                trusted_keys,
                plan,
                download_progress_callback,
                message_callback,
                progress_callback,
            )
            .await?;
        let prepared = PreparedInstall::new(installed, self.take_workspace()?);
        PackageReplacer::new(self.paths)
            .install_new(
                package_database,
                prepared,
                add_entry,
                message_callback,
                progress_callback,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn materialize_install<F, H, P>(
        &mut self,
        trusted_keys: &TrustedSignatureKeys,
        plan: InstallPlan,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        match plan.source {
            PlannedInstallSource::Release { release, asset } => {
                self.install_selected_asset(
                    trusted_keys,
                    plan.package,
                    &release,
                    &asset,
                    &plan.add_entry,
                    plan.trust_mode,
                    download_progress_callback,
                    message_callback,
                    progress_callback,
                )
                .await
            }
            PlannedInstallSource::LocalArtifact {
                artifact_path,
                version,
            } => {
                self.install_local_artifact(
                    plan.package,
                    &artifact_path,
                    version,
                    &plan.add_entry,
                    message_callback,
                    progress_callback,
                )
                .await
            }
        }
    }

    pub(crate) fn ensure_name_available(
        &self,
        database: &PackageDatabase,
        name: &str,
    ) -> Result<()> {
        if database.package_exists(name)? {
            return Err(anyhow!("Package '{}' already exists", name));
        }
        Ok(())
    }

    fn package_cache_key(package_name: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let sanitized = package_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();

        format!("{}-{}", sanitized, timestamp)
    }

    pub fn new(provider_manager: &'a ProviderManager, paths: &'a UpstreamPaths) -> Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let temp_path = std::env::temp_dir().join(format!("upstream-{nonce}"));
        let download_cache = temp_path.join("downloads");
        let extract_cache = temp_path.join("extracts");

        fs::create_dir_all(&download_cache).context(format!(
            "Failed to create download cache directory at '{}'",
            download_cache.display()
        ))?;
        fs::create_dir_all(&extract_cache).context(format!(
            "Failed to create extraction cache directory at '{}'",
            extract_cache.display()
        ))?;
        let workspace = InstallWorkspace::new(paths, "install")?;

        Ok(Self {
            provider_manager,
            paths,
            workspace: Some(workspace),
            download_cache,
            extract_cache,
        })
    }

    pub(crate) fn new_for_workspace(
        provider_manager: &'a ProviderManager,
        paths: &'a UpstreamPaths,
        workspace: InstallWorkspace,
    ) -> Result<Self> {
        let nonce = Self::package_cache_key("installer");
        let temp_path = std::env::temp_dir().join(format!("upstream-{nonce}"));
        let download_cache = temp_path.join("downloads");
        let extract_cache = temp_path.join("extracts");
        fs::create_dir_all(&download_cache).context(format!(
            "Failed to create download cache directory at '{}'",
            download_cache.display()
        ))?;
        fs::create_dir_all(&extract_cache).context(format!(
            "Failed to create extraction cache directory at '{}'",
            extract_cache.display()
        ))?;
        Ok(Self {
            provider_manager,
            paths,
            workspace: Some(workspace),
            download_cache,
            extract_cache,
        })
    }

    fn add_runtime_link(&self, _exec_path: &Path, _package_name: &str) -> Result<()> {
        Ok(())
    }

    fn workspace(&self) -> &InstallWorkspace {
        self.workspace
            .as_ref()
            .expect("installer workspace has not been initialized")
    }

    pub(crate) fn take_workspace(&mut self) -> Result<InstallWorkspace> {
        self.workspace
            .take()
            .ok_or_else(|| anyhow!("Installer workspace has already been consumed"))
    }

    fn install_completions_from_root<H>(
        &self,
        package_name: &str,
        root: &Path,
        message_callback: &mut Option<H>,
    ) where
        H: FnMut(&str),
    {
        if let Err(err) = CompletionManager::with_paths(self.workspace().completions.clone())
            .install_from_root(package_name, root, message_callback)
        {
            message!(
                message_callback,
                "{}",
                style(format!("Completion install skipped: {err}")).yellow()
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn install_selected_asset<F, H, P>(
        &self,
        trusted_keys: &TrustedSignatureKeys,
        package: Package,
        release: &Release,
        asset: &Asset,
        add_entry: &bool,
        trust_mode: TrustMode,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        if package.install_path.is_some() {
            return Err(anyhow!("Package '{}' is already installed", package.name));
        }

        let package_name = package.name.clone();
        let installed_package = {
            let progress_callback = std::cell::RefCell::new(progress_callback.as_mut());
            let mut bridged_progress = Some(|event: PackageProgressEvent| {
                if let Some(cb) = progress_callback.borrow_mut().as_deref_mut() {
                    cb(event);
                }
            });
            let mut bridged_download_progress = Some(|downloaded: u64, total: u64| {
                if let Some(cb) = download_progress_callback.as_mut() {
                    cb(downloaded, total);
                }
                if let Some(cb) = progress_callback.borrow_mut().as_deref_mut() {
                    cb(PackageProgressEvent::Download { downloaded, total });
                }
            });

            self.install_package_asset_files(
                package,
                release,
                asset,
                trust_mode,
                trusted_keys,
                &mut bridged_download_progress,
                message_callback,
                &mut bridged_progress,
            )
            .await
        }
        .context(format!(
            "Failed to perform installation for '{}'",
            package_name
        ))?;

        self.finish_installed_package(
            installed_package,
            add_entry,
            message_callback,
            progress_callback,
        )
        .await
    }

    pub async fn install_local_artifact<H, P>(
        &self,
        package: Package,
        artifact_path: &Path,
        version: crate::models::common::version::Version,
        add_entry: &bool,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        let installed_package = self
            .install_local_artifact_files(package, artifact_path, version, message_callback)
            .context("Failed to install local artifact")?;

        self.finish_installed_package(
            installed_package,
            add_entry,
            message_callback,
            progress_callback,
        )
        .await
    }

    pub async fn preview_single_install(
        &self,
        package: &Package,
        version: &Option<String>,
        semver: &Option<String>,
    ) -> Result<ResolvedAssetInstall> {
        if package.install_path.is_some() {
            return Err(anyhow!("Package '{}' is already installed", package.name));
        }

        let release = if let Some(version_tag) = version {
            self.provider_manager
                .get_release_by_tag(
                    &package.repo_slug,
                    version_tag,
                    &package.provider,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch release '{}' for '{}'. Verify the version tag exists",
                    version_tag, package.repo_slug
                ))?
        } else if let Some(semver) = semver {
            let requested = Version::parse(semver)
                .with_context(|| format!("Invalid semantic version '{semver}'"))?;
            self.provider_manager
                .get_release_by_semver(
                    &package.repo_slug,
                    &requested,
                    &package.provider,
                    &package.channel,
                    package.base_url.as_deref(),
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to resolve semantic version '{}' for '{}'",
                        semver, package.repo_slug
                    )
                })?
        } else {
            self.provider_manager
                .get_latest_release(
                    &package.repo_slug,
                    &package.provider,
                    &package.channel,
                    package.base_url.as_deref(),
                )
                .await
                .context(format!(
                    "Failed to fetch latest {} release for '{}'",
                    package.channel, package.repo_slug
                ))?
        };

        let best_asset = self
            .select_asset(package, &release, None::<&mut fn(&str)>)
            .await?;

        let resolved_filetype = if package.filetype == Filetype::Auto {
            best_asset.filetype
        } else {
            package.filetype
        };

        Ok(ResolvedAssetInstall {
            release_name: release.name.clone(),
            release_tag: release.tag.clone(),
            asset_name: best_asset.name.clone(),
            resolved_filetype,
            disk_impact: install_impact_from_download(asset_size_estimate(best_asset.size)),
            release,
            asset: best_asset.clone(),
        })
    }

    async fn finish_installed_package<H, P>(
        &self,
        installed_package: Package,
        _add_entry: &bool,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        let _ = message_callback;
        let _ = progress_callback;
        Ok(installed_package)
    }

    pub async fn resolve_release_asset<H>(
        &self,
        package: &Package,
        release: &Release,
        message_callback: Option<&mut H>,
    ) -> Result<Asset>
    where
        H: FnMut(&str),
    {
        self.select_asset(package, release, message_callback).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn install_package_asset_files<F, H, P>(
        &self,
        mut package: Package,
        release: &Release,
        asset: &Asset,
        trust_mode: TrustMode,
        trusted_keys: &TrustedSignatureKeys,
        download_progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        let cache_key = Self::package_cache_key(&package.name);
        let package_download_cache = self.download_cache.join(&cache_key);
        let package_extract_cache = self.extract_cache.join(&cache_key);
        fs::create_dir_all(&package_download_cache).context(format!(
            "Failed to create package download cache '{}'",
            package_download_cache.display()
        ))?;
        fs::create_dir_all(&package_extract_cache).context(format!(
            "Failed to create package extraction cache '{}'",
            package_extract_cache.display()
        ))?;

        let resolved_asset = self
            .try_resolve_selected_asset(
                &package,
                asset,
                &package_download_cache,
                message_callback.as_mut(),
            )
            .await?;
        let asset = resolved_asset.as_ref().unwrap_or(asset);

        if package.filetype == Filetype::Auto {
            message!(
                message_callback,
                "Resolved filetype to '{}'",
                &asset.filetype
            );
            package.filetype = asset.filetype;
        }

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::DownloadingPackage)
        );

        let download_path = self
            .provider_manager
            .download_asset(
                asset,
                &package.provider,
                &package_download_cache,
                download_progress_callback,
            )
            .await
            .context(format!("Failed to download asset '{}'", asset.name))?;

        let trust_verifier = TrustVerifier::new(
            self.provider_manager,
            &package_download_cache,
            trust_mode,
            trusted_keys,
        );

        let mut verifier_download_progress: Option<fn(u64, u64)> = None;
        let status = trust_verifier
            .verify_file(
                &download_path,
                release,
                &package.provider,
                &mut verifier_download_progress,
                message_callback,
                progress_callback,
            )
            .await
            .context("Failed trust verification")?;

        match status {
            TrustVerificationStatus::Skipped => {
                message!(
                    message_callback,
                    "{}",
                    style("Skipping checksum/signature verification (--trust none)").yellow()
                );
            }
            TrustVerificationStatus::Verified {
                checksum,
                signature,
            } => {
                match checksum {
                    ChecksumVerificationStatus::NotChecked => {}
                    ChecksumVerificationStatus::Verified => {
                        message!(message_callback, "{}", style("Checksum verified").green());
                    }
                    ChecksumVerificationStatus::Missing => {
                        if matches!(trust_mode, TrustMode::Signature | TrustMode::All) {
                            message!(
                                message_callback,
                                "{}",
                                style("Checksum missing (warning)").yellow()
                            );
                        } else {
                            message!(
                                message_callback,
                                "{}",
                                style("No checksum available").yellow()
                            );
                        }
                    }
                }

                match signature {
                    SignatureVerificationStatus::NotChecked => {}
                    SignatureVerificationStatus::Verified {
                        scheme,
                        key_id,
                        signature_asset,
                    } => {
                        let scheme_name = match scheme {
                            SignatureScheme::Minisign => "minisign",
                            SignatureScheme::Cosign => "cosign",
                        };
                        if let Some(id) = key_id {
                            message!(
                                message_callback,
                                "{}",
                                style(format!(
                                    "{} signature verified with key '{}'",
                                    scheme_name, id
                                ))
                                .green()
                            );
                        } else {
                            message!(
                                message_callback,
                                "{}",
                                style(format!("{scheme_name} signature verified")).green()
                            );
                        }
                        if !signature_asset.is_empty() {
                            message!(
                                message_callback,
                                "Verified against signature asset '{}'",
                                signature_asset
                            );
                        }
                    }
                    SignatureVerificationStatus::MissingSignature => {
                        if matches!(trust_mode, TrustMode::Checksum | TrustMode::All) {
                            message!(
                                message_callback,
                                "{}",
                                style("Signature missing (warning)").yellow()
                            );
                        } else {
                            message!(
                                message_callback,
                                "{}",
                                style("No signature available").yellow()
                            );
                        }
                    }
                    SignatureVerificationStatus::InvalidSignature
                    | SignatureVerificationStatus::NoTrustedKeyMatched => {}
                }
            }
        }

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::InstallingPackage)
        );

        package.record_release(release);
        let package_name = package.name.clone();
        let package_provider = package.provider.clone();

        let installed_package = match package.filetype {
            Filetype::AppImage => {
                #[cfg(target_os = "linux")]
                {
                    self.handle_appimage(&download_path, package, message_callback)
                        .await
                        .context("Failed to install AppImage")
                }
                #[cfg(not(target_os = "linux"))]
                {
                    anyhow::bail!("AppImage installation is only supported on Linux hosts");
                }
            }
            Filetype::Compressed => {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Phase(PackagePhase::ExtractingPackage)
                );
                self.handle_compressed(
                    &download_path,
                    &package_extract_cache,
                    package,
                    message_callback,
                )
                .context("Failed to install compressed file")
            }
            Filetype::Archive => {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Phase(PackagePhase::ExtractingPackage)
                );
                self.handle_archive(
                    &download_path,
                    &package_extract_cache,
                    package,
                    message_callback,
                )
                .context("Failed to install archive")
            }
            _ => {
                progress!(
                    progress_callback,
                    PackageProgressEvent::Phase(PackagePhase::CreatingRuntimeLinks)
                );
                self.handle_file(&download_path, package, message_callback)
                    .context("Failed to install file")
            }
        }?;
        self.finish_verified_release_install(
            installed_package,
            &package_name,
            &package_provider,
            release,
            &package_download_cache,
            message_callback,
            progress_callback,
        )
        .await
    }

    /// Complete a release artifact that has already been downloaded, verified,
    /// and materialized into this installer's temporary workspace.
    ///
    /// This is shared by the normal download path and zsync so that staged
    /// installs always receive identical cleanup and completion handling.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finish_verified_release_install<H, P>(
        &self,
        installed_package: Package,
        package_name: &str,
        package_provider: &crate::models::common::enums::Provider,
        release: &Release,
        artifact_cache: &Path,
        message_callback: &mut Option<H>,
        progress_callback: &mut Option<P>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
        P: FnMut(PackageProgressEvent),
    {
        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::InstallingCompletions)
        );
        if let Err(err) = CompletionManager::with_paths(self.workspace().completions.clone())
            .install_from_release_assets(
                package_name,
                release,
                self.provider_manager,
                package_provider,
                artifact_cache,
                message_callback,
            )
            .await
        {
            progress!(
                progress_callback,
                PackageProgressEvent::Warning(format!("Completion install skipped: {err}"))
            );
        }
        Ok(installed_package)
    }

    pub fn install_local_artifact_files<H>(
        &self,
        mut package: Package,
        artifact_path: &Path,
        version: crate::models::common::version::Version,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        if !artifact_path.exists() {
            return Err(anyhow!(
                "Local artifact path '{}' does not exist",
                artifact_path.display()
            ));
        }

        message!(message_callback, "Installing local artifact ...");
        package.version = version;

        if artifact_path.is_dir() {
            return self
                .handle_archive(
                    artifact_path,
                    &self.extract_cache,
                    package,
                    message_callback,
                )
                .context("Failed to install local artifact directory");
        }

        self.handle_file(artifact_path, package, message_callback)
            .context("Failed to install local artifact file")
    }

    fn handle_archive<H>(
        &self,
        asset_path: &Path,
        extract_cache: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid archive path: no filename"))?
            .to_string_lossy()
            .to_string();
        message!(message_callback, "Extracting directory '{filename}' ...");

        let extracted_path = compression_handler::decompress(asset_path, extract_cache)
            .context(format!("Failed to extract archive '{}'", filename))?;

        if extracted_path.is_file() {
            return self.handle_file(&extracted_path, package, message_callback);
        }

        let dirname = extracted_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = self.workspace().archives_dir.join(dirname);
        let install_root = archive_layout::select_nested_archive_root(&extracted_path, &package)
            .unwrap_or_else(|| extracted_path.clone());

        message!(
            message_callback,
            "Moving directory to '{}' ...",
            out_path.display()
        );

        safe_move::move_file_or_dir(&install_root, &out_path).context(format!(
            "Failed to move extracted directory from '{}' to '{}'",
            install_root.display(),
            out_path.display()
        ))?;

        message!(message_callback, "Searching for executable ...");

        let Some(exec_path) = permission_handler::find_executable(&out_path, &package.name) else {
            message!(
                message_callback,
                "{}",
                style("Could not automatically locate executable").yellow()
            );
            self.install_completions_from_root(&package.name, &out_path, message_callback);
            package.exec_path = None;
            package.install_path = Some(out_path);
            package.last_upgraded = Utc::now();
            return Ok(package);
        };

        permission_handler::make_executable(&exec_path).context(format!(
            "Failed to make '{}' executable",
            exec_path.display()
        ))?;

        message!(
            message_callback,
            "Added executable permission for '{}'",
            exec_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| exec_path.display().to_string())
        );

        self.add_runtime_link(&exec_path, &package.name)?;
        if false {
            message!(
                message_callback,
                "Created symlink: {} → {}",
                package.name,
                out_path.display()
            );
        }

        self.install_completions_from_root(&package.name, &out_path, message_callback);
        package.exec_path = Some(exec_path);
        package.install_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }

    async fn select_asset<H>(
        &self,
        package: &Package,
        release: &Release,
        message_callback: Option<&mut H>,
    ) -> Result<Asset>
    where
        H: FnMut(&str),
    {
        if let Some(descriptor) = dotslash_parser::find_asset(release, package) {
            let cache_key = format!("{}-dotslash", Self::package_cache_key(&package.name));
            let descriptor_cache = self.download_cache.join(cache_key);
            fs::create_dir_all(&descriptor_cache).context(format!(
                "Failed to create DotSlash download cache '{}'",
                descriptor_cache.display()
            ))?;

            if let Some(asset) = self
                .resolve_asset(package, descriptor, &descriptor_cache, message_callback)
                .await?
            {
                return Ok(asset);
            }
        }

        self.find_recommended_asset(release, package)
            .context(format!(
                "Could not find a compatible asset for '{}' (filetype: {:?}, arch: detected automatically)",
                package.name, package.filetype
            ))
    }

    fn find_recommended_asset(&self, release: &Release, package: &Package) -> Result<Asset> {
        let mut filtered_release = release.clone();
        filtered_release
            .assets
            .retain(|asset| !dotslash_parser::is_asset(&asset.name, package));

        self.provider_manager
            .find_recommended_asset(&filtered_release, package)
    }

    async fn try_resolve_selected_asset<H>(
        &self,
        package: &Package,
        asset: &Asset,
        package_download_cache: &Path,
        message_callback: Option<&mut H>,
    ) -> Result<Option<Asset>>
    where
        H: FnMut(&str),
    {
        dotslash_parser::resolve_selected_asset(
            package,
            asset,
            self.provider_manager,
            package_download_cache,
            message_callback,
        )
        .await
    }

    async fn resolve_asset<H>(
        &self,
        package: &Package,
        asset: &Asset,
        download_cache: &Path,
        message_callback: Option<&mut H>,
    ) -> Result<Option<Asset>>
    where
        H: FnMut(&str),
    {
        dotslash_parser::resolve_asset(
            package,
            asset,
            self.provider_manager,
            download_cache,
            message_callback,
        )
        .await
    }

    fn handle_compressed<H>(
        &self,
        asset_path: &Path,
        extract_cache: &Path,
        package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid compressed path: no filename"))?
            .to_string_lossy()
            .to_string();
        message!(message_callback, "Extracting file '{}' ...", filename);

        let extracted_path = compression_handler::decompress(asset_path, extract_cache)
            .context(format!("Failed to decompress '{}'", filename))?;

        self.handle_file(&extracted_path, package, message_callback)
    }

    #[cfg(target_os = "linux")]
    async fn handle_appimage<H>(
        &self,
        asset_path: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = self.workspace().appimages_dir.join(filename);

        message!(
            message_callback,
            "Moving file to '{}' ...",
            out_path.display()
        );

        safe_move::move_file_or_dir(asset_path, &out_path).context(format!(
            "Failed to move AppImage to '{}'",
            out_path.display()
        ))?;

        permission_handler::make_executable(&out_path).context(format!(
            "Failed to make AppImage '{}' executable",
            filename.to_string_lossy()
        ))?;

        message!(message_callback, "Made '{}' executable", filename.display());

        let completion_root = match crate::services::artifact::AppImageExtractor::new() {
            Ok(extractor) => match extractor
                .extract(&package.name, &out_path, message_callback)
                .await
            {
                Ok(root) => Some(root),
                Err(err) => {
                    message!(
                        message_callback,
                        "{}",
                        style(format!("AppImage completion scan skipped: {err}")).yellow()
                    );
                    None
                }
            },
            Err(err) => {
                message!(
                    message_callback,
                    "{}",
                    style(format!("AppImage completion scan skipped: {err}")).yellow()
                );
                None
            }
        };

        self.add_runtime_link(&out_path, &package.name)?;
        if false {
            message!(
                message_callback,
                "Created symlink: {} → {}",
                package.name,
                out_path.display()
            );
        }

        if let Some(root) = completion_root {
            self.install_completions_from_root(&package.name, &root, message_callback);
        }

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }

    fn handle_file<H>(
        &self,
        asset_path: &Path,
        mut package: Package,
        message_callback: &mut Option<H>,
    ) -> Result<Package>
    where
        H: FnMut(&str),
    {
        let filename = asset_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = self.workspace().binaries_dir.join(filename);

        message!(
            message_callback,
            "Moving file to '{}' ...",
            out_path.display()
        );

        safe_move::move_file_or_dir(asset_path, &out_path)
            .context(format!("Failed to move binary to '{}'", out_path.display()))?;

        permission_handler::make_executable(&out_path).context(format!(
            "Failed to make binary '{}' executable",
            filename.to_string_lossy()
        ))?;

        message!(message_callback, "Made '{}' executable", filename.display());

        self.add_runtime_link(&out_path, &package.name)?;
        if false {
            message!(
                message_callback,
                "Created symlink: {} → {}",
                package.name,
                out_path.display()
            );
        }

        package.install_path = Some(out_path.clone());
        package.exec_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }
}

impl<'a> Drop for PackageInstaller<'a> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.extract_cache);
        let _ = fs::remove_dir_all(&self.download_cache);
    }
}

#[cfg(test)]
mod tests {
    use super::{InstallWorkspace, PackageInstaller, archive_layout};
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::providers::provider_manager::ProviderManager;
    use crate::utils::test_support;
    use std::fs;

    fn make_package(
        name: &str,
        match_pattern: Option<&str>,
        exclude_pattern: Option<&str>,
    ) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Archive,
            match_pattern.map(str::to_string),
            exclude_pattern.map(str::to_string),
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[cfg(target_os = "linux")]
    fn host_linux_gnu_dir() -> Option<&'static str> {
        if cfg!(target_arch = "x86_64") {
            Some("x86_64-unknown-linux-gnu")
        } else if cfg!(target_arch = "x86") {
            Some("x86_32-unknown-linux-gnu")
        } else if cfg!(target_arch = "aarch64") {
            Some("aarch64-unknown-linux-gnu")
        } else if cfg!(target_arch = "arm") {
            Some("armv7-unknown-linux-gnueabihf")
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn host_linux_glibc_dir() -> Option<&'static str> {
        if cfg!(target_arch = "x86_64") {
            Some("x86_64-unknown-linux-gnu-glibc2.28")
        } else if cfg!(target_arch = "x86") {
            Some("x86_32-unknown-linux-gnu-glibc2.28")
        } else if cfg!(target_arch = "aarch64") {
            Some("aarch64-unknown-linux-gnu-glibc2.28")
        } else if cfg!(target_arch = "arm") {
            Some("armv7-unknown-linux-gnueabihf-glibc2.28")
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn host_linux_musl_dir() -> Option<&'static str> {
        if cfg!(target_arch = "x86_64") {
            Some("x86_64-unknown-linux-musl")
        } else if cfg!(target_arch = "x86") {
            Some("x86_32-unknown-linux-musl")
        } else if cfg!(target_arch = "aarch64") {
            Some("aarch64-unknown-linux-musl")
        } else if cfg!(target_arch = "arm") {
            Some("armv7-unknown-linux-musleabihf")
        } else {
            None
        }
    }

    #[test]
    fn package_cache_key_sanitizes_disallowed_characters() {
        let key = PackageInstaller::package_cache_key("my/pkg v1.0");
        assert!(key.starts_with("my_pkg_v1_0-"));
        assert!(
            key.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn staged_install_writes_only_to_its_workspace() {
        let root = test_support::temp_root("upstream-installer-test", "staged-workspace");
        let paths = test_support::upstream_paths(&root);
        let source_dir = root.join("source");
        fs::create_dir_all(&source_dir).expect("create source");
        let artifact = source_dir.join("tool-bin");
        fs::write(&artifact, b"new binary").expect("write artifact");

        let workspace = InstallWorkspace::new(&paths, "tool").expect("workspace");
        let staged_root = workspace.root().to_path_buf();
        let provider_manager =
            ProviderManager::new(None, None, None, Default::default()).expect("provider manager");
        let installer = PackageInstaller::new_for_workspace(&provider_manager, &paths, workspace)
            .expect("staged installer");
        let mut package = make_package("tool", None, None);
        package.filetype = Filetype::Binary;
        let mut no_messages: Option<fn(&str)> = None;

        let installed = installer
            .install_local_artifact_files(
                package,
                &artifact,
                crate::models::common::Version::new(1, 0, 0, false),
                &mut no_messages,
            )
            .expect("stage local artifact");

        let staged_path = staged_root.join("binaries/tool-bin");
        assert_eq!(
            installed.install_path.as_deref(),
            Some(staged_path.as_path())
        );
        assert!(staged_path.exists());
        assert!(!paths.install.binaries_dir.join("tool-bin").exists());
        assert!(!paths.state.symlinks_dir.join("tool").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn nested_archive_root_prefers_host_linux_gnu_payload() {
        let Some(expected_dir) = host_linux_gnu_dir() else {
            return;
        };
        let root = test_support::temp_root("upstream-installer-test", "nested-broot");
        let extracted = root.join("broot_1.56.4");
        fs::create_dir_all(&extracted).expect("create extracted root");

        for dir in [
            "x86_64-pc-windows-gnu",
            "x86_64-unknown-linux-musl",
            "x86_64-unknown-linux-gnu-glibc2.28",
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "aarch64-unknown-linux-musl",
            "armv7-unknown-linux-gnueabihf",
            "armv7-unknown-linux-musleabihf",
        ] {
            let payload = extracted.join(dir);
            fs::create_dir_all(&payload).expect("create payload");
            fs::write(
                payload.join(if dir.contains("windows") {
                    "broot.exe"
                } else {
                    "broot"
                }),
                b"bin",
            )
            .expect("write payload binary");
        }

        fs::create_dir_all(extracted.join("completion")).expect("create completion");
        fs::write(extracted.join("broot.1"), b"manpage").expect("write manpage");

        let selected = archive_layout::select_nested_archive_root(
            &extracted,
            &make_package("broot", None, None),
        )
        .expect("select nested root");

        assert!(selected.ends_with(expected_dir));

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn nested_archive_root_honors_match_and_exclude_patterns() {
        let (Some(musl_dir), Some(gnu_dir), Some(glibc_dir)) = (
            host_linux_musl_dir(),
            host_linux_gnu_dir(),
            host_linux_glibc_dir(),
        ) else {
            return;
        };
        let root = test_support::temp_root("upstream-installer-test", "nested-patterns");
        let extracted = root.join("tool_1.0.0");
        fs::create_dir_all(&extracted).expect("create extracted root");

        for dir in [musl_dir, gnu_dir, glibc_dir] {
            let payload = extracted.join(dir);
            fs::create_dir_all(&payload).expect("create payload");
            fs::write(payload.join("tool"), b"bin").expect("write payload binary");
        }

        let selected_musl = archive_layout::select_nested_archive_root(
            &extracted,
            &make_package("tool", Some("musl"), None),
        )
        .expect("select musl root");
        assert!(selected_musl.ends_with(musl_dir));

        let selected_glibc = archive_layout::select_nested_archive_root(
            &extracted,
            &make_package("tool", None, Some("linux-gnu")),
        )
        .expect("select non-excluded root");
        assert!(selected_glibc.ends_with(musl_dir));

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn nested_archive_root_ignores_ordinary_archive_layouts() {
        let root = test_support::temp_root("upstream-installer-test", "ordinary-archive");
        let extracted = root.join("tool_1.0.0");
        fs::create_dir_all(extracted.join("bin")).expect("create bin");
        fs::write(extracted.join("bin").join("tool"), b"bin").expect("write binary");
        fs::create_dir_all(extracted.join("docs")).expect("create docs");

        assert!(
            archive_layout::select_nested_archive_root(
                &extracted,
                &make_package("tool", None, None),
            )
            .is_none()
        );

        fs::remove_dir_all(&root).expect("cleanup");
    }
}
