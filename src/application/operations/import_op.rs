use crate::{
    application::cancellation,
    models::{
        common::enums::TrustMode,
        upstream::{InstallType, Package, PackageReference, config::AppConfig},
    },
    providers::provider_manager::ProviderManager,
    routines::build::{BuildRequest, scripts::BuildScriptAction, worker::BuildWorker},
    services::{
        integration::ShellManager,
        packaging::{OperationPhase, PackageInstaller, PackagePhase, PackageProgressEvent},
        trust::{CosignPublicKey, MinisignPublicKey, TrustedSignatureKeys},
    },
    storage::{
        database::PackageDatabase,
        system::{config::ConfigStorage, trust::TrustStorage},
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow, bail};
use futures_util::stream::{FuturesUnordered, StreamExt};
use minisign_verify::PublicKey;
use p256::ecdsa::VerifyingKey;
use p256::pkcs8::DecodePublicKey;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::time::{self, Duration};

pub const PACKAGES_EXPORT_VERSION: u32 = 2;
pub const PROFILE_EXPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportPackageResult {
    Installed { version: String },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportProgressEvent {
    Phase(OperationPhase),
    Detail(String),
    Started {
        package_width: usize,
    },
    Overall {
        completed: u32,
        total: u32,
    },
    Package {
        name: String,
        event: PackageProgressEvent,
    },
    Warning {
        name: String,
        message: String,
    },
    Complete {
        name: String,
        result: ImportPackageResult,
    },
    Clear,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub installed: u32,
    pub skipped: u32,
    pub failed: u32,
}

type ProgressState = Arc<Mutex<BTreeMap<String, PackageProgressEvent>>>;
type WarningState = Arc<Mutex<Vec<(String, String)>>>;

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
    install_concurrency: usize,
}

impl<'a> ImportOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_database: &'a mut PackageDatabase,
        paths: &'a UpstreamPaths,
        trusted_keys: TrustedSignatureKeys,
        install_concurrency: usize,
    ) -> Self {
        Self {
            provider_manager,
            package_database,
            paths,
            trusted_keys,
            install_concurrency: install_concurrency.max(1),
        }
    }

    pub async fn import_packages<P>(
        &mut self,
        path: &Path,
        skip_failed: bool,
        latest: bool,
        progress_callback: &mut Option<P>,
    ) -> Result<ImportSummary>
    where
        P: FnMut(ImportProgressEvent),
    {
        let packages = Self::read_packages(path)?;
        self.import_packages_from_export(packages, skip_failed, latest, progress_callback)
            .await
    }

    pub fn import_keys<P>(&self, path: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(ImportProgressEvent),
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
    ) -> Result<ImportSummary>
    where
        P: FnMut(ImportProgressEvent),
    {
        let profile = Self::read_profile(path)?;
        self.install_concurrency = profile.config.concurrency.install_concurrency();
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
    ) -> Result<ImportSummary>
    where
        P: FnMut(ImportProgressEvent),
    {
        let total = export.packages.len() as u32;
        let mut summary = ImportSummary::default();
        let mut completed = 0_u32;
        emit_phase(progress_callback, OperationPhase::ImportingPackages);

        let mut queued_names = HashSet::new();
        let mut eligible = Vec::new();
        for reference in export.packages {
            cancellation::check()?;
            let mut skipped = false;
            if self.package_database.package_exists(&reference.name)? {
                summary.skipped += 1;
                skipped = true;
                emit_warning(
                    progress_callback,
                    &reference.name,
                    "already exists; skipping",
                );
            } else if !queued_names.insert(reference.name.clone()) {
                summary.skipped += 1;
                skipped = true;
                emit_warning(
                    progress_callback,
                    &reference.name,
                    "is duplicated in the export; skipping",
                );
            } else {
                let version = if latest {
                    None
                } else {
                    reference.version_tag.clone()
                };
                eligible.push((reference.into_package(), version));
            }
            if skipped {
                completed += 1;
                emit_overall(progress_callback, completed, total);
            }
        }

        emit_overall(progress_callback, completed, total);

        let installer = PackageInstaller::new(self.provider_manager, self.paths)?;
        if let Some(cb) = progress_callback.as_mut() {
            cb(ImportProgressEvent::Started {
                package_width: eligible
                    .iter()
                    .map(|(package, _)| package.name.chars().count())
                    .max()
                    .unwrap_or(0),
            });
        }
        let progress_state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let warning_state: WarningState = Arc::new(Mutex::new(Vec::new()));
        let mut last_progress_events = BTreeMap::new();
        let mut pending = FuturesUnordered::new();
        let mut packages = eligible.into_iter();
        let mut stop_scheduling = false;
        let mut first_error = None;

        for _ in 0..self.install_concurrency {
            let Some((package, version)) = packages.next() else {
                break;
            };
            pending.push(import_package(
                &installer,
                self.trusted_keys.clone(),
                package,
                version,
                Arc::clone(&progress_state),
                Arc::clone(&warning_state),
            ));
        }

        let mut ticker = time::interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
        while !pending.is_empty() {
            cancellation::check()?;
            tokio::select! {
                maybe_result = pending.next() => {
                    let Some((name, result)) = maybe_result else { break };
                    emit_progress_updates(&progress_state, &warning_state, &mut last_progress_events, progress_callback);
                    if let Ok(mut state) = progress_state.lock() {
                        state.remove(&name);
                    }
                    last_progress_events.remove(&name);

                    match result.and_then(|package| self.persist_imported_package(&installer, &package).map(|()| package)) {
                        Ok(package) => {
                            summary.installed += 1;
                            emit_complete(progress_callback, name, ImportPackageResult::Installed { version: package.version.to_string() });
                        }
                        Err(err) => {
                            summary.failed += 1;
                            emit_complete(progress_callback, name.clone(), ImportPackageResult::Failed { error: crate::output::error_summary(&err) });
                            if !skip_failed && first_error.is_none() {
                                stop_scheduling = true;
                                first_error = Some(err.context(format!("Failed to import package '{name}'")));
                            }
                        }
                    }
                    completed += 1;
                    emit_overall(progress_callback, completed, total);

                    if !stop_scheduling
                        && let Some((package, version)) = packages.next() {
                            pending.push(import_package(
                                &installer,
                                self.trusted_keys.clone(),
                                package,
                                version,
                                Arc::clone(&progress_state),
                                Arc::clone(&warning_state),
                            ));
                        }
                }
                _ = ticker.tick() => {
                    emit_progress_updates(&progress_state, &warning_state, &mut last_progress_events, progress_callback);
                }
            }
        }

        emit_progress_updates(
            &progress_state,
            &warning_state,
            &mut last_progress_events,
            progress_callback,
        );
        if let Some(cb) = progress_callback.as_mut() {
            cb(ImportProgressEvent::Clear);
        }
        if let Some(err) = first_error {
            return Err(err);
        }

        emit_detail(
            progress_callback,
            format!(
                "Packages import complete: {} installed, {} skipped, {} failed",
                summary.installed, summary.skipped, summary.failed
            ),
        );
        Ok(summary)
    }

    fn persist_imported_package(
        &mut self,
        installer: &PackageInstaller<'_>,
        installed_package: &Package,
    ) -> Result<()> {
        if let Err(err) = self.package_database.upsert_package(installed_package) {
            return self.cleanup_after_metadata_error(installer, installed_package, err);
        }

        if let Err(err) = ShellManager::new(&self.paths.config.paths_file)
            .regenerate_paths(self.package_database, self.paths)
        {
            let _ = self
                .package_database
                .remove_package(&installed_package.name);
            return self.cleanup_after_metadata_error(installer, installed_package, err);
        }

        Ok(())
    }

    fn cleanup_after_metadata_error(
        &self,
        installer: &PackageInstaller<'_>,
        installed_package: &Package,
        err: anyhow::Error,
    ) -> Result<()> {
        let mut ignored_messages = Some(|_: &str| {});
        match installer.cleanup_partial_install(installed_package, &mut ignored_messages) {
            Ok(()) => Err(err.context(format!(
                "Rolled back partial install for '{}'",
                installed_package.name
            ))),
            Err(cleanup_err) => Err(anyhow!(
                "{}. Additionally failed to roll back partial install for '{}': {}",
                err,
                installed_package.name,
                cleanup_err
            )),
        }
    }

    fn import_key_values<P>(
        &self,
        minisign_keys: Vec<MinisignPublicKey>,
        cosign_keys: Vec<CosignPublicKey>,
        progress_callback: &mut Option<P>,
    ) -> Result<()>
    where
        P: FnMut(ImportProgressEvent),
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
        P: FnMut(ImportProgressEvent),
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
    P: FnMut(ImportProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(ImportProgressEvent::Phase(phase));
    }
}

fn emit_warning<P>(progress_callback: &mut Option<P>, name: &str, message: impl Into<String>)
where
    P: FnMut(ImportProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(ImportProgressEvent::Warning {
            name: name.to_string(),
            message: message.into(),
        });
    }
}

fn emit_detail<P>(progress_callback: &mut Option<P>, message: impl Into<String>)
where
    P: FnMut(ImportProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(ImportProgressEvent::Detail(message.into()));
    }
}

fn emit_overall<P>(progress_callback: &mut Option<P>, completed: u32, total: u32)
where
    P: FnMut(ImportProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(ImportProgressEvent::Overall { completed, total });
    }
}

fn emit_complete<P>(progress_callback: &mut Option<P>, name: String, result: ImportPackageResult)
where
    P: FnMut(ImportProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(ImportProgressEvent::Complete { name, result });
    }
}

fn record_progress_event(
    progress_state: &ProgressState,
    warning_state: &WarningState,
    name: &str,
    event: PackageProgressEvent,
) {
    if let PackageProgressEvent::Warning(message) = event {
        if let Ok(mut warnings) = warning_state.lock() {
            warnings.push((name.to_string(), message));
        }
        return;
    }
    if let Ok(mut state) = progress_state.lock() {
        state.insert(name.to_string(), event);
    }
}

fn emit_progress_updates<P>(
    progress_state: &ProgressState,
    warning_state: &WarningState,
    last_progress_events: &mut BTreeMap<String, PackageProgressEvent>,
    progress_callback: &mut Option<P>,
) where
    P: FnMut(ImportProgressEvent),
{
    let warnings = warning_state
        .lock()
        .map(|mut warnings| warnings.drain(..).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(cb) = progress_callback.as_mut() {
        for (name, message) in warnings {
            cb(ImportProgressEvent::Warning { name, message });
        }
    }

    let snapshot = progress_state
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default();
    for (name, event) in &snapshot {
        let changed = last_progress_events
            .get(name)
            .map(|previous| previous != event)
            .unwrap_or(true);
        if changed {
            if let Some(cb) = progress_callback.as_mut() {
                cb(ImportProgressEvent::Package {
                    name: name.clone(),
                    event: event.clone(),
                });
            }
            last_progress_events.insert(name.clone(), event.clone());
        }
    }
}

async fn import_package(
    installer: &PackageInstaller<'_>,
    trusted_keys: TrustedSignatureKeys,
    package: Package,
    version: Option<String>,
    progress_state: ProgressState,
    warning_state: WarningState,
) -> (String, Result<Package>) {
    let name = package.name.clone();
    let progress_name = name.clone();
    let mut progress_callback = Some(move |event: PackageProgressEvent| {
        record_progress_event(&progress_state, &warning_state, &progress_name, event);
    });
    let result = match package.install_type {
        InstallType::Release => {
            let mut no_download_progress: Option<fn(u64, u64)> = None;
            let mut ignored_messages = Some(|_: &str| {});
            installer
                .install_release(
                    &trusted_keys,
                    package,
                    &version,
                    &false,
                    TrustMode::BestEffort,
                    &mut no_download_progress,
                    &mut ignored_messages,
                    &mut progress_callback,
                )
                .await
        }
        InstallType::Build => {
            import_build_package(installer, package, version, &mut progress_callback).await
        }
    }
    .context(format!("Failed to import package '{name}'"));
    (name, result)
}

async fn import_build_package<P>(
    installer: &PackageInstaller<'_>,
    mut package: Package,
    version_tag: Option<String>,
    progress_callback: &mut Option<P>,
) -> Result<Package>
where
    P: FnMut(PackageProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(PackageProgressEvent::Phase(
            PackagePhase::RebuildingFromSource,
        ));
    }
    let worker = BuildWorker::new(installer.provider_manager(), installer.paths());
    let mut ignored_build_lines = Some(|_: &str| {});
    let output = worker
        .build(
            build_request_for_import(&package, version_tag),
            package.channel.clone(),
            &mut ignored_build_lines,
        )
        .await?;
    package.build_branch = output.branch.clone();
    package.build_commit = output.commit.clone();
    if package.build_branch.is_none() {
        package.record_release(&output.release);
    }

    let mut ignored_messages = Some(|_: &str| {});
    installer
        .install_local_artifact(
            package,
            &output.artifact_path,
            output.version,
            &false,
            &mut ignored_messages,
            progress_callback,
        )
        .await
}

fn build_request_for_import(package: &Package, version_tag: Option<String>) -> BuildRequest {
    BuildRequest {
        name: package.name.clone(),
        repo_slug: package.repo_slug.clone(),
        provider: package.provider.clone(),
        base_url: package.base_url.clone(),
        version_tag: if package.build_branch.is_some() {
            None
        } else {
            version_tag
        },
        branch: package.build_branch.clone(),
        requested_profile: None,
        script_action: BuildScriptAction::Install,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ImportOperation, ImportProgressEvent, PACKAGES_EXPORT_VERSION, PROFILE_EXPORT_VERSION,
        ProgressState, WarningState, emit_progress_updates, record_progress_event,
    };
    use crate::services::packaging::{PackagePhase, PackageProgressEvent};
    use crate::{
        models::{
            common::enums::{Channel, Filetype, Provider},
            upstream::{InstallType, Package},
        },
        routines::build::scripts::BuildScriptAction,
    };
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        sync::{Arc, Mutex},
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

        assert_eq!(config.download.low_threads, 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn progress_updates_emit_each_latest_package_event_once() {
        let state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let warnings: WarningState = Arc::new(Mutex::new(Vec::new()));
        let mut last_render = BTreeMap::new();
        let mut events = Vec::new();

        record_progress_event(
            &state,
            &warnings,
            "ripgrep",
            PackageProgressEvent::Phase(PackagePhase::InstallingPackage),
        );
        {
            let mut callback = Some(|event: ImportProgressEvent| events.push(event));
            emit_progress_updates(&state, &warnings, &mut last_render, &mut callback);
        }
        assert!(events.iter().any(|event| matches!(
            event,
            ImportProgressEvent::Package {
                name,
                event: PackageProgressEvent::Phase(PackagePhase::InstallingPackage),
            } if name == "ripgrep"
        )));

        {
            let mut callback = Some(|event: ImportProgressEvent| events.push(event));
            emit_progress_updates(&state, &warnings, &mut last_render, &mut callback);
        }
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn warning_progress_is_emitted_separately() {
        let state: ProgressState = Arc::new(Mutex::new(BTreeMap::new()));
        let warnings: WarningState = Arc::new(Mutex::new(Vec::new()));
        let mut last_render = BTreeMap::new();
        let mut events = Vec::new();

        record_progress_event(
            &state,
            &warnings,
            "ripgrep",
            PackageProgressEvent::Warning("signature unavailable".to_string()),
        );
        let mut callback = Some(|event: ImportProgressEvent| events.push(event));
        emit_progress_updates(&state, &warnings, &mut last_render, &mut callback);

        assert!(matches!(
            events.as_slice(),
            [ImportProgressEvent::Warning { name, message }]
                if name == "ripgrep" && message == "signature unavailable"
        ));
    }

    #[test]
    fn build_import_uses_exported_release_tag_or_branch() {
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
        package.install_type = InstallType::Build;

        let release_request = super::build_request_for_import(&package, Some("v1.2.3".to_string()));
        assert_eq!(release_request.version_tag.as_deref(), Some("v1.2.3"));
        assert!(release_request.branch.is_none());
        assert_eq!(release_request.script_action, BuildScriptAction::Install);

        package.build_branch = Some("main".to_string());
        let branch_request = super::build_request_for_import(&package, Some("v1.2.3".to_string()));
        assert!(branch_request.version_tag.is_none());
        assert_eq!(branch_request.branch.as_deref(), Some("main"));
    }
}
