use crate::{
    models::upstream::PackageReference,
    services::packaging::{OperationPhase, OperationProgressEvent},
    services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::Serialize;
use std::path::PathBuf;
use std::{fs, path::Path};
use tar::Builder;
use walkdir::WalkDir;

/// The manifest written by a light export.
#[derive(Serialize)]
pub struct ExportManifest {
    pub version: u32,
    pub exported_at: String,
    pub packages: Vec<PackageReference>,
}

pub struct ExportOperation<'a> {
    package_storage: &'a PackageStorage,
    paths: &'a UpstreamPaths,
}

impl<'a> ExportOperation<'a> {
    pub fn new(package_storage: &'a PackageStorage, paths: &'a UpstreamPaths) -> Self {
        Self {
            package_storage,
            paths,
        }
    }

    /// Light export: write a JSON manifest of PackageReferences.
    pub fn export_manifest<P>(&self, output: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        let packages = self.package_storage.get_all_packages();

        let references: Vec<PackageReference> = packages
            .iter()
            .filter(|p| p.install_path.is_some())
            .map(|p| PackageReference::from_package(p.clone()))
            .collect();

        if references.is_empty() {
            return Err(anyhow!("No installed packages to export"));
        }

        if let Some(cb) = progress_callback.as_mut() {
            cb(OperationProgressEvent::Phase(
                OperationPhase::SerializingManifest,
            ));
        }

        let manifest = ExportManifest {
            version: 1,
            exported_at: chrono::Utc::now().to_rfc3339(),
            packages: references,
        };

        let json =
            serde_json::to_string_pretty(&manifest).context("Failed to serialise manifest")?;

        if let Some(cb) = progress_callback.as_mut() {
            cb(OperationProgressEvent::Phase(
                OperationPhase::WritingManifest,
            ));
        }

        fs::write(output, json).context(format!(
            "Failed to write manifest to '{}'",
            output.display()
        ))
    }

    /// Full export: tarball the entire .upstream directory.
    pub fn export_snapshot<P>(&self, output: &Path, progress_callback: &mut Option<P>) -> Result<()>
    where
        P: FnMut(OperationProgressEvent),
    {
        let upstream_dir = &self.paths.dirs.data_dir;

        if !upstream_dir.exists() {
            return Err(anyhow!(
                "Upstream directory '{}' does not exist — nothing to snapshot",
                upstream_dir.display()
            ));
        }

        if let Some(cb) = progress_callback.as_mut() {
            cb(OperationProgressEvent::Phase(OperationPhase::ScanningFiles));
        }

        // Collect entries first to get deterministic total
        let entries: Vec<_> = WalkDir::new(upstream_dir)
            .follow_links(false)
            .into_iter()
            .collect::<Result<_, _>>()
            .context("Failed while walking upstream directory")?;

        let total_files = entries.iter().filter(|e| e.file_type().is_file()).count() as u64;

        if let Some(cb) = progress_callback.as_mut() {
            cb(OperationProgressEvent::Count {
                done: 0,
                total: total_files,
            });
        }

        let file = fs::File::create(output).context(format!(
            "Failed to create archive at '{}'",
            output.display()
        ))?;

        let gz = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(gz);

        let mut processed = 0u64;

        for entry in entries {
            let path = entry.path();

            let rel_path = path
                .strip_prefix(upstream_dir)
                .context("Failed to compute relative path")?;

            let mut archive_path = PathBuf::from("upstream");
            if !rel_path.as_os_str().is_empty() {
                archive_path.push(rel_path);
            }

            if entry.file_type().is_dir() {
                tar.append_dir(&archive_path, path)
                    .context(format!("Failed to append directory '{}'", path.display()))?;
            } else if entry.file_type().is_file() {
                if let Some(cb) = progress_callback.as_mut() {
                    cb(OperationProgressEvent::Detail(format!(
                        "Archiving {}",
                        rel_path.display()
                    )));
                }

                let mut file = fs::File::open(path)
                    .context(format!("Failed to open file '{}'", path.display()))?;

                tar.append_file(&archive_path, &mut file)
                    .context(format!("Failed to append file '{}'", path.display()))?;

                processed += 1;

                if let Some(cb) = progress_callback.as_mut() {
                    cb(OperationProgressEvent::Count {
                        done: processed,
                        total: total_files,
                    });
                }
            }
        }

        if let Some(cb) = progress_callback.as_mut() {
            cb(OperationProgressEvent::Phase(
                OperationPhase::FinalizingArchive,
            ));
        }

        tar.finish().context("Failed to finalise snapshot archive")
    }
}

#[cfg(test)]
mod tests {
    use super::ExportOperation;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::services::storage::package_storage::PackageStorage;
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
    fn export_manifest_fails_when_no_installed_packages_exist() {
        let root = temp_root("empty");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let operation = ExportOperation::new(&storage, &paths);
        let output = root.join("manifest.json");
        let mut progress: Option<fn(crate::services::packaging::OperationProgressEvent)> = None;

        let err = operation
            .export_manifest(&output, &mut progress)
            .expect_err("no installed packages");
        assert!(err.to_string().contains("No installed packages"));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn export_manifest_writes_installed_package_references() {
        let root = temp_root("manifest");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let mut pkg = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        pkg.install_path = Some(paths.install.binaries_dir.join("tool"));
        pkg.build_branch = Some("main".to_string());
        pkg.build_commit = Some("abc123".to_string());
        storage
            .add_or_update_package(pkg)
            .expect("store installed package");

        let operation = ExportOperation::new(&storage, &paths);
        let output = root.join("manifest.json");
        let mut progress: Option<fn(crate::services::packaging::OperationProgressEvent)> = None;
        operation
            .export_manifest(&output, &mut progress)
            .expect("export manifest");

        let content = fs::read_to_string(&output).expect("read manifest");
        assert!(content.contains("\"version\": 1"));
        assert!(content.contains("\"name\": \"tool\""));
        assert!(content.contains("\"repo_slug\": \"owner/tool\""));
        assert!(content.contains("\"build_branch\": \"main\""));
        assert!(content.contains("\"build_commit\": \"abc123\""));

        cleanup(&root).expect("cleanup");
    }
}
