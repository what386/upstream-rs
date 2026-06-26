use crate::{
    application::operations::install_op::{InstallOperation, ReleaseInstallRequest},
    models::{
        common::enums::TrustMode,
        upstream::{AppConfig, InstallType, PackageReference},
    },
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{OperationPhase, OperationProgressEvent, PackageProgressEvent},
        trust::{CosignPublicKey, MinisignPublicKey, TrustedSignatureKeys},
    },
    storage::{
        database::PackageDatabase,
        system::{config::ConfigStorage, trust::TrustStorage},
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, bail};
use minisign_verify::PublicKey;
use p256::ecdsa::VerifyingKey;
use p256::pkcs8::DecodePublicKey;
use serde::Deserialize;
use std::{fs, path::Path};

pub const PACKAGES_EXPORT_VERSION: u32 = 2;
pub const PROFILE_EXPORT_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
pub struct ImportPackages {
    pub version: u32,
    pub packages: Vec<PackageReference>,
}

#[derive(Deserialize)]
struct ImportKeys {
    version: u32,
    #[serde(default)]
    minisign_public_keys: Vec<MinisignPublicKey>,
    #[serde(default)]
    cosign_public_keys: Vec<CosignPublicKey>,
}

#[derive(Deserialize)]
struct ImportProfile {
    version: u32,
    config: AppConfig,
    packages: ImportPackages,
    keys: ImportKeys,
}

pub struct ImportOperation<'a> {
    provider_manager: &'a ProviderManager,
    package_database: &'a mut PackageDatabase,
    paths: &'a UpstreamPaths,
    trusted_keys: TrustedSignatureKeys,
}

impl<'a> ImportOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_database: &'a mut PackageDatabase,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
    ) -> Self {
        Self {
            provider_manager,
            package_database,
            paths,
            trusted_keys,
        }
    }

    pub async fn import_packages<P>(
        &mut self,
        path: &Path,
        skip_failed: bool,
        latest: bool,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        let packages = Self::read_packages(path)?;
        self.import_packages_from_export(packages, skip_failed, latest, progress_callback)
            .await
    }

    pub fn import_keys<P>(&self, path: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        let (minisign_keys, cosign_keys) = match Self::read_keys_export(path) {
            Ok(keys) => (keys.minisign_public_keys, keys.cosign_public_keys),
            Err(_) => Self::parse_signature_key_file(path)?,
        };
        self.import_key_values(minisign_keys, cosign_keys, progress_callback)
    }

    pub fn read_profile_config(path: &Path) -> Result<AppConfig> {
        Ok(Self::read_profile(path)?.config)
    }

    pub async fn import_profile<P>(
        &mut self,
        path: &Path,
        skip_failed: bool,
        latest: bool,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        let profile = Self::read_profile(path)?;
        self.import_config_value(profile.config, progress_callback)?;
        self.import_key_values(
            profile.keys.minisign_public_keys,
            profile.keys.cosign_public_keys,
            progress_callback,
        )?;
        self.trusted_keys =
            TrustStorage::new(&self.paths.config.trust_file)?.trusted_signature_keys();
        self.import_packages_from_export(profile.packages, skip_failed, latest, progress_callback)
            .await
    }

    fn read_packages(path: &Path) -> Result<ImportPackages> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read packages export from '{}'", path.display()))?;
        let packages: ImportPackages =
            serde_json::from_str(&content).context("Failed to parse packages export")?;
        if packages.version != PACKAGES_EXPORT_VERSION {
            bail!(
                "Unsupported packages export version {}. Upgrade upstream and try again.",
                packages.version
            );
        }
        Ok(packages)
    }

    fn read_keys_export(path: &Path) -> Result<ImportKeys> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read keys export from '{}'", path.display()))?;
        let keys: ImportKeys =
            serde_json::from_str(&content).context("Failed to parse keys export")?;
        if keys.version != crate::storage::system::trust::TRUST_STORAGE_VERSION {
            bail!(
                "Unsupported keys export version {}. Upgrade upstream and try again.",
                keys.version
            );
        }
        Ok(keys)
    }

    fn read_profile(path: &Path) -> Result<ImportProfile> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read profile export from '{}'", path.display()))?;
        let profile: ImportProfile =
            serde_json::from_str(&content).context("Failed to parse profile export")?;
        if profile.version != PROFILE_EXPORT_VERSION {
            bail!(
                "Unsupported profile export version {}. Upgrade upstream and try again.",
                profile.version
            );
        }
        Self::validate_packages(&profile.packages)?;
        Self::validate_keys(&profile.keys)?;
        Ok(profile)
    }

    fn validate_packages(packages: &ImportPackages) -> Result<()> {
        if packages.version != PACKAGES_EXPORT_VERSION {
            bail!(
                "Unsupported packages export version {}. Upgrade upstream and try again.",
                packages.version
            );
        }
        Ok(())
    }

    fn validate_keys(keys: &ImportKeys) -> Result<()> {
        if keys.version != crate::storage::system::trust::TRUST_STORAGE_VERSION {
            bail!(
                "Unsupported keys export version {}. Upgrade upstream and try again.",
                keys.version
            );
        }
        Ok(())
    }

    async fn import_packages_from_export<P>(
        &mut self,
        export: ImportPackages,
        skip_failed: bool,
        latest: bool,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        let total = export.packages.len() as u32;
        let mut completed = 0_u32;
        let mut imported = 0_u32;
        let mut skipped = 0_u32;
        emit_phase(progress_callback, OperationPhase::ImportingPackages);

        for reference in export.packages {
            if self.package_database.package_exists(&reference.name)? {
                skipped += 1;
                emit_warning(
                    progress_callback,
                    format!("Package '{}' already exists; skipping", reference.name),
                );
            } else if reference.install_type != InstallType::Release {
                skipped += 1;
                emit_warning(
                    progress_callback,
                    format!(
                        "Package '{}' is a build package; build imports are not supported",
                        reference.name
                    ),
                );
            } else {
                let package_name = reference.name.clone();
                let version = if latest {
                    None
                } else {
                    reference.version_tag.clone()
                };
                let result = {
                    let mut install_operation = InstallOperation::new(
                        self.provider_manager,
                        self.package_database,
                        self.paths,
                        self.trusted_keys.clone(),
                    )?;
                    let mut no_download_progress: Option<fn(u64, u64)> = None;
                    let mut ignored_messages = Some(|_: &str| {});
                    let mut package_progress = None::<fn(PackageProgressEvent)>;
                    install_operation
                        .install_release(
                            ReleaseInstallRequest {
                                package: reference.into_package(),
                                version,
                                add_entry: false,
                                trust_mode: TrustMode::BestEffort,
                            },
                            &mut no_download_progress,
                            &mut ignored_messages,
                            &mut package_progress,
                        )
                        .await
                };

                if let Err(err) = result {
                    if skip_failed {
                        skipped += 1;
                        emit_warning(
                            progress_callback,
                            format!("Failed to import package '{}': {err}", package_name),
                        );
                    } else {
                        return Err(err).with_context(|| {
                            format!("Failed to import package '{}'", package_name)
                        });
                    }
                } else {
                    imported += 1;
                }
            }

            completed += 1;
            if let Some(cb) = progress_callback.as_mut() {
                cb(OperationProgressEvent::Count {
                    done: completed.into(),
                    total: total.into(),
                });
            }
        }

        emit_detail(
            progress_callback,
            format!(
                "Packages import complete: {} installed, {} skipped",
                imported, skipped
            ),
        );
        Ok(())
    }

    fn import_key_values<P>(
        &self,
        minisign_keys: Vec<MinisignPublicKey>,
        cosign_keys: Vec<CosignPublicKey>,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        emit_phase(progress_callback, OperationPhase::ImportingKeys);
        let mut trust_storage = TrustStorage::new(&self.paths.config.trust_file)?;
        let summary = trust_storage.merge_trusted_keys(&minisign_keys, &cosign_keys)?;
        emit_detail(
            progress_callback,
            format!(
                "Key import complete: minisign {} imported, {} deduped, {} total; cosign {} imported, {} deduped, {} total",
                summary.minisign.imported,
                summary.minisign.deduped,
                summary.minisign.total,
                summary.cosign.imported,
                summary.cosign.deduped,
                summary.cosign.total
            ),
        );
        Ok(())
    }

    fn import_config_value<P>(
        &self,
        config: AppConfig,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        emit_phase(progress_callback, OperationPhase::ImportingConfig);
        let mut target = ConfigStorage::new(&self.paths.config.config_file)?;
        target.replace_config(config)?;
        emit_detail(progress_callback, "Config import complete");
        Ok(())
    }

    fn parse_signature_key_file(
        path: &Path,
    ) -> Result<(Vec<MinisignPublicKey>, Vec<CosignPublicKey>)> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read key file '{}'", path.display()))?;
        let mut minisign_keys = Vec::new();
        let mut cosign_keys = Vec::new();
        let mut in_pem = false;
        let mut pem_lines: Vec<String> = Vec::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.to_ascii_lowercase().starts_with("untrusted comment:") {
                continue;
            }
            if PublicKey::from_base64(line).is_ok() {
                minisign_keys.push(MinisignPublicKey {
                    id: None,
                    key: line.to_string(),
                });
                continue;
            }

            if line.contains("BEGIN PUBLIC KEY") {
                in_pem = true;
                pem_lines.clear();
            }

            if in_pem {
                pem_lines.push(raw_line.to_string());
                if line.contains("END PUBLIC KEY") {
                    in_pem = false;
                    let pem = pem_lines.join("\n");
                    if VerifyingKey::from_public_key_pem(&pem).is_ok() {
                        cosign_keys.push(CosignPublicKey { id: None, key: pem });
                    }
                    pem_lines.clear();
                }
            }
        }

        if minisign_keys.is_empty() && cosign_keys.is_empty() {
            bail!(
                "No valid minisign or cosign public keys found in '{}'",
                path.display()
            );
        }

        Ok((minisign_keys, cosign_keys))
    }
}

fn emit_phase<P>(progress_callback: &mut Option<P>, phase: OperationPhase)
where
    P: FnMut(OperationProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(OperationProgressEvent::Phase(phase));
    }
}

fn emit_warning<P>(progress_callback: &mut Option<P>, message: impl Into<String>)
where
    P: FnMut(OperationProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(OperationProgressEvent::Warning(message.into()));
    }
}

fn emit_detail<P>(progress_callback: &mut Option<P>, message: impl Into<String>)
where
    P: FnMut(OperationProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(OperationProgressEvent::Detail(message.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::{ImportOperation, PACKAGES_EXPORT_VERSION, PROFILE_EXPORT_VERSION};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-import-test-{name}-{nanos}.json"))
    }

    #[test]
    fn read_packages_rejects_unsupported_version() {
        let path = temp_file("bad-version");
        fs::write(&path, r#"{"version":1,"packages":[]}"#).expect("write packages");

        let err = ImportOperation::read_packages(&path).expect_err("reject old version");

        assert!(
            err.to_string()
                .contains("Unsupported packages export version")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_packages_accepts_current_version() {
        let path = temp_file("current-version");
        fs::write(
            &path,
            format!(r#"{{"version":{PACKAGES_EXPORT_VERSION},"packages":[]}}"#),
        )
        .expect("write packages");

        let packages = ImportOperation::read_packages(&path).expect("read packages");

        assert_eq!(packages.version, PACKAGES_EXPORT_VERSION);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_keys_export_accepts_current_version() {
        let path = temp_file("keys-current-version");
        fs::write(
            &path,
            format!(
                r#"{{"version":{},"minisign_public_keys":[{{"id":"fixture","key":"abc"}}],"cosign_public_keys":[]}}"#,
                crate::storage::system::trust::TRUST_STORAGE_VERSION
            ),
        )
        .expect("write keys");

        let keys = ImportOperation::read_keys_export(&path).expect("read keys");

        assert_eq!(keys.minisign_public_keys.len(), 1);
        assert_eq!(keys.minisign_public_keys[0].id.as_deref(), Some("fixture"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_profile_config_accepts_current_version() {
        let path = temp_file("profile-current-version");
        fs::write(
            &path,
            format!(
                r#"{{
                    "version":{PROFILE_EXPORT_VERSION},
                    "config":{{}},
                    "packages":{{"version":{PACKAGES_EXPORT_VERSION},"packages":[]}},
                    "keys":{{"version":{},"minisign_public_keys":[],"cosign_public_keys":[]}}
                }}"#,
                crate::storage::system::trust::TRUST_STORAGE_VERSION
            ),
        )
        .expect("write profile");

        let config = ImportOperation::read_profile_config(&path).expect("read profile config");

        assert!(config.github.api_token.is_none());
        let _ = fs::remove_file(path);
    }
}
