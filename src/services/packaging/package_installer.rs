use crate::{
    models::common::enums::TrustMode,
    models::{common::enums::Filetype, provider::Release, upstream::Package},
    providers::provider_manager::ProviderManager,
    services::{
        integration::{
            CompletionManager, ShellManager, SymlinkManager, compression_handler,
            permission_handler,
        },
        packaging::{PackagePhase, PackageProgressEvent, bundle_handler::BundleHandler},
        trust::{
            ChecksumVerificationStatus, SignatureScheme, SignatureVerificationStatus,
            TrustVerificationStatus, TrustVerifier, TrustedSignatureKeys,
        },
    },
    utils::{filesystem::safe_move, static_paths::UpstreamPaths},
};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use console::style;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::utils::{
    filename_parser::{parse_arch, parse_os},
    platform::platform_info::{ArchitectureInfo, CpuArch, OSKind},
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

pub struct PackageInstaller<'a> {
    provider_manager: &'a ProviderManager,
    paths: &'a UpstreamPaths,
    download_cache: PathBuf,
    extract_cache: PathBuf,
}

impl<'a> PackageInstaller<'a> {
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
        let temp_path = std::env::temp_dir().join(format!("upstream-{}", std::process::id()));
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
            download_cache,
            extract_cache,
        })
    }

    /// Install package files from a release
    /// Returns the updated package with installation paths set
    pub async fn install_package_files<F, H, P>(
        &self,
        mut package: Package,
        release: &Release,
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

        message!(message_callback, "Selecting asset from '{}'", release.name);

        let best_asset = self
            .provider_manager
            .find_recommended_asset(release, &package)
            .context(format!(
                "Could not find a compatible asset for '{}' (filetype: {:?}, arch: detected automatically)",
                package.name, package.filetype
            ))?;

        if package.filetype == Filetype::Auto {
            message!(
                message_callback,
                "Resolved filetype to '{}'",
                &best_asset.filetype
            );
            package.filetype = best_asset.filetype;
        }

        progress!(
            progress_callback,
            PackageProgressEvent::Phase(PackagePhase::DownloadingPackage)
        );

        let download_path = self
            .provider_manager
            .download_asset(
                &best_asset,
                &package.provider,
                &package_download_cache,
                download_progress_callback,
            )
            .await
            .context(format!("Failed to download asset '{}'", best_asset.name))?;

        let trust_verifier = TrustVerifier::new(
            self.provider_manager,
            &package_download_cache,
            trust_mode,
            trusted_keys,
        );
        let status = trust_verifier
            .verify_file(
                &download_path,
                release,
                &package.provider,
                download_progress_callback,
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
            PackageProgressEvent::Phase(PackagePhase::InstallingCompletions)
        );
        if let Err(err) = CompletionManager::new(self.paths)
            .install_from_release_assets(
                &package.name,
                release,
                self.provider_manager,
                &package.provider,
                &package_download_cache,
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

        package.version = release.version.clone();

        match package.filetype {
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
            Filetype::MacApp => BundleHandler::new(self.paths, &self.extract_cache)
                .install_app_bundle(&download_path, package, message_callback)
                .context("Failed to install macOS app bundle"),
            Filetype::MacDmg => BundleHandler::new(self.paths, &self.extract_cache)
                .install_dmg(&download_path, package, message_callback)
                .context("Failed to install macOS disk image"),
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
        }
    }

    pub fn install_local_artifact<H>(
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

        if let Err(err) = CompletionManager::new(self.paths).install_from_root(
            &package.name,
            &extracted_path,
            message_callback,
        ) {
            message!(
                message_callback,
                "{}",
                style(format!("Completion install skipped: {err}")).yellow()
            );
        }

        if let Some(app_bundle_path) =
            BundleHandler::find_macos_app_bundle(&extracted_path, &package.name)
                .context("Failed to detect .app bundle in extracted archive")?
        {
            return BundleHandler::new(self.paths, &self.extract_cache)
                .install_app_bundle(&app_bundle_path, package, message_callback)
                .context("Failed to install app bundle from archive");
        }

        let dirname = extracted_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path: no filename"))?;
        let out_path = self.paths.install.archives_dir.join(dirname);
        let install_root = Self::select_nested_archive_root(&extracted_path, &package)
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

        let shell_manager = ShellManager::new(&self.paths.config.paths_file);

        message!(message_callback, "Searching for executable ...");

        let Some(exec_path) = permission_handler::find_executable(&out_path, &package.name) else {
            message!(
                message_callback,
                "{}",
                style("Could not automatically locate executable").yellow()
            );
            // Fallback: add out_path to PATH
            shell_manager
                .add_to_paths(&out_path)
                .context(format!("Failed to add '{}' to PATH", out_path.display()))?;
            message!(message_callback, "Added '{}' to PATH", out_path.display());
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

        let path_to_add = exec_path
            .parent()
            .ok_or_else(|| anyhow!("Executable has no parent directory"))?;

        shell_manager
            .add_to_paths(path_to_add)
            .context(format!("Failed to add '{}' to PATH", path_to_add.display()))?;

        message!(
            message_callback,
            "Added '{}' to PATH",
            path_to_add.display()
        );

        let symlink_manager = SymlinkManager::new(&self.paths.integration.symlinks_dir);

        symlink_manager
            .add_link(&exec_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

        package.exec_path = Some(exec_path);
        package.install_path = Some(out_path);
        package.last_upgraded = Utc::now();
        Ok(package)
    }

    fn select_nested_archive_root(extracted_path: &Path, package: &Package) -> Option<PathBuf> {
        if !extracted_path.is_dir() {
            return None;
        }

        let architecture = ArchitectureInfo::new();
        let mut candidates = fs::read_dir(extracted_path)
            .ok()?
            .flatten()
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_dir() {
                    return None;
                }

                let name = entry.file_name().to_string_lossy().to_string();
                let target_os = parse_os(&name)?;
                let target_arch = parse_arch(&name)?;

                if target_os != architecture.os_kind {
                    return None;
                }

                let lower = name.to_ascii_lowercase();
                if let Some(pattern) = package.exclude_pattern.as_deref()
                    && lower.contains(&pattern.to_ascii_lowercase())
                {
                    return None;
                }

                let arch_score = Self::nested_arch_score(&architecture.cpu_arch, &target_arch)?;
                permission_handler::find_executable(&entry.path(), &package.name)?;
                let score = Self::nested_archive_score(
                    &name,
                    &target_os,
                    arch_score,
                    package.match_pattern.as_deref(),
                );

                Some((score, name, entry.path()))
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return None;
        }

        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        candidates.into_iter().next().map(|(_, _, path)| path)
    }

    fn nested_arch_score(host_arch: &CpuArch, target_arch: &CpuArch) -> Option<i32> {
        if host_arch == target_arch {
            return Some(100);
        }

        if *host_arch == CpuArch::X86_64 && *target_arch == CpuArch::X86 {
            return Some(40);
        }

        if *host_arch == CpuArch::Aarch64 && *target_arch == CpuArch::Arm {
            return Some(40);
        }

        None
    }

    fn nested_archive_score(
        name: &str,
        target_os: &OSKind,
        arch_score: i32,
        match_pattern: Option<&str>,
    ) -> i32 {
        let lower = name.to_ascii_lowercase();
        let mut score = arch_score;

        if *target_os == OSKind::Linux {
            score += Self::linux_abi_score(&lower);
        }

        if let Some(pattern) = match_pattern
            && lower.contains(&pattern.to_ascii_lowercase())
        {
            score += 100;
        }

        score
    }

    fn linux_abi_score(name: &str) -> i32 {
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        {
            if name.contains("musl") {
                return 30;
            }
            if name.contains("gnu") || name.contains("glibc") {
                return 10;
            }
            return 0;
        }

        #[cfg(all(target_os = "linux", not(target_env = "musl")))]
        {
            if name.contains("linux-gnu") && !name.contains("glibc") {
                return 30;
            }
            if name.contains("glibc") {
                return 20;
            }
            if name.contains("musl") {
                return 10;
            }
            0
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = name;
            0
        }
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
        let out_path = self.paths.install.appimages_dir.join(filename);

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

        match crate::services::integration::AppImageExtractor::new() {
            Ok(extractor) => match extractor
                .extract(&package.name, &out_path, message_callback)
                .await
            {
                Ok(root) => {
                    if let Err(err) = CompletionManager::new(self.paths).install_from_root(
                        &package.name,
                        &root,
                        message_callback,
                    ) {
                        message!(
                            message_callback,
                            "{}",
                            style(format!("Completion install skipped: {err}")).yellow()
                        );
                    }
                }
                Err(err) => {
                    message!(
                        message_callback,
                        "{}",
                        style(format!("AppImage completion scan skipped: {err}")).yellow()
                    );
                }
            },
            Err(err) => {
                message!(
                    message_callback,
                    "{}",
                    style(format!("AppImage completion scan skipped: {err}")).yellow()
                );
            }
        }

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .add_link(&out_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

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
        let out_path = self.paths.install.binaries_dir.join(filename);

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

        SymlinkManager::new(&self.paths.integration.symlinks_dir)
            .add_link(&out_path, &package.name)
            .context(format!("Failed to create symlink for '{}'", package.name))?;

        message!(
            message_callback,
            "Created symlink: {} → {}",
            package.name,
            out_path.display()
        );

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
    use super::PackageInstaller;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
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

        let selected = PackageInstaller::select_nested_archive_root(
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

        let selected_musl = PackageInstaller::select_nested_archive_root(
            &extracted,
            &make_package("tool", Some("musl"), None),
        )
        .expect("select musl root");
        assert!(selected_musl.ends_with(musl_dir));

        let selected_glibc = PackageInstaller::select_nested_archive_root(
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
            PackageInstaller::select_nested_archive_root(
                &extracted,
                &make_package("tool", None, None),
            )
            .is_none()
        );

        fs::remove_dir_all(&root).expect("cleanup");
    }
}
