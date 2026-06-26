use crate::{
    application::operations::import_op::{PACKAGES_EXPORT_VERSION, PROFILE_EXPORT_VERSION},
    models::upstream::{AppConfig, PackageReference},
    services::{
        packaging::{OperationPhase, OperationProgressEvent},
        trust::{CosignPublicKey, MinisignPublicKey},
    },
    storage::{
        database::PackageDatabase,
        system::{config::ConfigStorage, trust::TrustStorage},
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Serialize)]
pub struct PackagesExport {
    pub version: u32,
    pub exported_at: String,
    pub packages: Vec<PackageReference>,
}

#[derive(Serialize)]
struct KeysExport {
    version: u32,
    exported_at: String,
    minisign_public_keys: Vec<MinisignPublicKey>,
    cosign_public_keys: Vec<CosignPublicKey>,
}

#[derive(Serialize)]
struct ProfileExport {
    version: u32,
    exported_at: String,
    config: AppConfig,
    packages: PackagesExport,
    keys: KeysExport,
}

pub struct ExportOperation<'a> {
    package_database: &'a PackageDatabase,
    paths: &'a UpstreamPaths,
}

impl<'a> ExportOperation<'a> {
    pub fn new(package_database: &'a PackageDatabase, paths: &'a UpstreamPaths) -> Self {
        Self {
            package_database,
            paths,
        }
    }

    pub fn export_packages<P>(&self, output: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        let references = self.package_references()?;
        if references.is_empty() {
            return Err(anyhow!("No installed packages to export"));
        }

        emit_phase(progress_callback, OperationPhase::SerializingExport);
        let export = packages_export(references, chrono::Utc::now().to_rfc3339());
        write_json(output, &export, "packages export", progress_callback)
    }

    pub fn export_keys<P>(&self, output: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        emit_phase(progress_callback, OperationPhase::SerializingExport);
        let export = self.keys_export(chrono::Utc::now().to_rfc3339())?;
        write_json(output, &export, "keys export", progress_callback)
    }

    pub fn export_config<P>(&self, output: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        emit_phase(progress_callback, OperationPhase::SerializingExport);
        let config_storage = ConfigStorage::new(&self.paths.config.config_file)?;
        let toml = toml::to_string_pretty(config_storage.get_config())
            .context("Failed to serialize config export")?;
        write_text(output, &toml, "config export", progress_callback)
    }

    pub fn export_profile<P>(&self, output: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        emit_phase(progress_callback, OperationPhase::SerializingExport);
        let exported_at = chrono::Utc::now().to_rfc3339();
        let config = ConfigStorage::new(&self.paths.config.config_file)?
            .get_config()
            .clone();
        let export = ProfileExport {
            version: PROFILE_EXPORT_VERSION,
            exported_at: exported_at.clone(),
            config,
            packages: packages_export(self.package_references()?, exported_at.clone()),
            keys: self.keys_export(exported_at)?,
        };
        write_json(output, &export, "profile export", progress_callback)
    }

    fn package_references(&self) -> Result<Vec<PackageReference>> {
        let packages = self.package_database.list_packages()?;
        Ok(packages
            .iter()
            .filter(|package| package.install_path.is_some())
            .map(|package| PackageReference::from_package(package.clone()))
            .collect())
    }

    fn keys_export(&self, exported_at: String) -> Result<KeysExport> {
        let keys = TrustStorage::new(&self.paths.config.trust_file)?.trusted_signature_keys();
        Ok(KeysExport {
            version: crate::storage::system::trust::TRUST_STORAGE_VERSION,
            exported_at,
            minisign_public_keys: keys.minisign_public_keys,
            cosign_public_keys: keys.cosign_public_keys,
        })
    }
}

fn packages_export(packages: Vec<PackageReference>, exported_at: String) -> PackagesExport {
    PackagesExport {
        version: PACKAGES_EXPORT_VERSION,
        exported_at,
        packages,
    }
}

fn write_json<P, T>(
    output: &Path,
    value: &T,
    label: &str,
    progress_callback: &mut Option<P>,
) -> Result<()>
where
    P: FnMut(OperationProgressEvent),
    T: Serialize,
{
    let json = serde_json::to_string_pretty(value)
        .with_context(|| format!("Failed to serialize {label}"))?;
    emit_phase(progress_callback, OperationPhase::WritingExport);
    fs::write(output, json)
        .with_context(|| format!("Failed to write {label} to '{}'", output.display()))
}

fn write_text<P>(
    output: &Path,
    value: &str,
    label: &str,
    progress_callback: &mut Option<P>,
) -> Result<()>
where
    P: FnMut(OperationProgressEvent),
{
    emit_phase(progress_callback, OperationPhase::WritingExport);
    fs::write(output, value)
        .with_context(|| format!("Failed to write {label} to '{}'", output.display()))
}

fn emit_phase<P>(progress_callback: &mut Option<P>, phase: OperationPhase)
where
    P: FnMut(OperationProgressEvent),
{
    if let Some(cb) = progress_callback.as_mut() {
        cb(OperationProgressEvent::Phase(phase));
    }
}

#[cfg(test)]
mod tests {
    use super::ExportOperation;
    use crate::application::operations::import_op::PACKAGES_EXPORT_VERSION;
    use crate::models::common::Version;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::storage::database::PackageDatabase;
    use crate::storage::system::config::ConfigStorage;
    use crate::utils::test_support;
    use std::path::Path;
    use std::{fs, io};

    fn temp_root(name: &str) -> std::path::PathBuf {
        test_support::temp_root("upstream-export-test", name)
    }

    fn test_paths(root: &Path) -> crate::utils::static_paths::UpstreamPaths {
        test_support::upstream_paths(root)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn export_packages_fails_when_no_installed_packages_exist() {
        let root = temp_root("empty");
        let paths = test_paths(&root);
        let storage = PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
        let operation = ExportOperation::new(&storage, &paths);
        let output = root.join("packages.json");
        let mut progress: Option<fn(crate::services::packaging::OperationProgressEvent)> = None;

        let err = operation
            .export_packages(&output, &mut progress)
            .expect_err("no installed packages");

        assert!(err.to_string().contains("No installed packages"));
        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn export_packages_writes_installed_package_references() {
        let root = temp_root("packages");
        let paths = test_paths(&root);
        let mut storage =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
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
        package.version = Version::new(1, 2, 3, false);
        package.version_tag_template = Some("v{}".to_string());
        package.install_path = Some(paths.install.binaries_dir.join("tool"));
        storage
            .upsert_package(&package)
            .expect("store installed package");

        let operation = ExportOperation::new(&storage, &paths);
        let output = root.join("packages.json");
        let mut progress: Option<fn(crate::services::packaging::OperationProgressEvent)> = None;
        operation
            .export_packages(&output, &mut progress)
            .expect("export packages");

        let content = fs::read_to_string(&output).expect("read packages");
        assert!(content.contains(&format!("\"version\": {PACKAGES_EXPORT_VERSION}")));
        assert!(content.contains("\"name\": \"tool\""));
        assert!(content.contains("\"repo_slug\": \"owner/tool\""));
        assert!(content.contains("\"version_tag\": \"v1.2.3\""));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn export_config_writes_config_toml() {
        let root = temp_root("config");
        let paths = test_paths(&root);
        ConfigStorage::new(&paths.config.config_file)
            .expect("config storage")
            .save_config()
            .expect("save config");
        let storage = PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
        let operation = ExportOperation::new(&storage, &paths);
        let output = root.join("config.toml");
        let mut progress: Option<fn(crate::services::packaging::OperationProgressEvent)> = None;

        operation
            .export_config(&output, &mut progress)
            .expect("export config");

        let content = fs::read_to_string(&output).expect("read config");
        assert!(!content.contains("version ="));
        assert!(content.contains("[download]"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn export_profile_writes_config_packages_and_keys() {
        let root = temp_root("profile");
        let paths = test_paths(&root);
        ConfigStorage::new(&paths.config.config_file)
            .expect("config storage")
            .save_config()
            .expect("save config");
        let mut storage =
            PackageDatabase::open(&paths.config.packages_database_file).expect("storage");
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
        package.version = Version::new(1, 2, 3, false);
        package.install_path = Some(paths.install.binaries_dir.join("tool"));
        storage.upsert_package(&package).expect("store package");
        let operation = ExportOperation::new(&storage, &paths);
        let output = root.join("profile.json");
        let mut progress: Option<fn(crate::services::packaging::OperationProgressEvent)> = None;

        operation
            .export_profile(&output, &mut progress)
            .expect("export profile");

        let content = fs::read_to_string(&output).expect("read profile");
        let profile: serde_json::Value = serde_json::from_str(&content).expect("parse profile");
        assert_eq!(profile["version"].as_u64(), Some(1));
        assert!(profile["config"]["version"].is_null());
        assert_eq!(
            profile["packages"]["packages"][0]["name"].as_str(),
            Some("tool")
        );
        assert_eq!(profile["keys"]["version"].as_u64(), Some(1));

        cleanup(&root).expect("cleanup");
    }
}
