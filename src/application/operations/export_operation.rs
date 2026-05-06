use crate::{
    models::upstream::PackageReference, services::storage::package_storage::PackageStorage,
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
    pub fn export_manifest<H>(&self, output: &Path, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
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

        if let Some(cb) = message_callback {
            cb("Serialising manifest...");
        }

        let manifest = ExportManifest {
            version: 1,
            exported_at: chrono::Utc::now().to_rfc3339(),
            packages: references,
        };

        let json =
            serde_json::to_string_pretty(&manifest).context("Failed to serialise manifest")?;

        if let Some(cb) = message_callback {
            cb("Writing manifest file...");
        }

        fs::write(output, json).context(format!(
            "Failed to write manifest to '{}'",
            output.display()
        ))
    }

    /// Full export: tarball the entire .upstream directory.
    pub fn export_snapshot<F, H>(
        &self,
        output: &Path,
        progress_callback: &mut Option<F>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        H: FnMut(&str),
    {
        let upstream_dir = &self.paths.dirs.data_dir;

        if !upstream_dir.exists() {
            return Err(anyhow!(
                "Upstream directory '{}' does not exist — nothing to snapshot",
                upstream_dir.display()
            ));
        }

        if let Some(cb) = message_callback {
            cb("Scanning files...");
        }

        // Collect entries first to get deterministic total
        let entries: Vec<_> = WalkDir::new(upstream_dir)
            .follow_links(false)
            .into_iter()
            .collect::<Result<_, _>>()
            .context("Failed while walking upstream directory")?;

        let total_files = entries.iter().filter(|e| e.file_type().is_file()).count() as u64;

        if let Some(cb) = progress_callback {
            cb(0, total_files);
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
                if let Some(cb) = message_callback {
                    cb(&format!("Archiving {}", rel_path.display()));
                }

                let mut file = fs::File::open(path)
                    .context(format!("Failed to open file '{}'", path.display()))?;

                tar.append_file(&archive_path, &mut file)
                    .context(format!("Failed to append file '{}'", path.display()))?;

                processed += 1;

                if let Some(cb) = progress_callback {
                    cb(processed, total_files);
                }
            }
        }

        if let Some(cb) = message_callback {
            cb("Finalising archive...");
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
    use crate::utils::static_paths::{
        AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths,
    };
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-export-test-{name}-{nanos}"))
    }

    fn test_paths(root: &Path) -> UpstreamPaths {
        let dirs = AppDirs {
            user_dir: root.to_path_buf(),
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            metadata_dir: root.join("data/metadata"),
        };

        UpstreamPaths {
            config: ConfigPaths {
                config_file: dirs.config_dir.join("config.toml"),
                packages_file: dirs.metadata_dir.join("packages.json"),
                metadata_file: dirs.metadata_dir.join("metadata.json"),
                paths_file: dirs.metadata_dir.join("paths.sh"),
            },
            install: InstallPaths {
                appimages_dir: dirs.data_dir.join("appimages"),
                binaries_dir: dirs.data_dir.join("binaries"),
                archives_dir: dirs.data_dir.join("archives"),
                rollback_dir: dirs.data_dir.join("rollback"),
            },
            integration: IntegrationPaths {
                symlinks_dir: dirs.data_dir.join("symlinks"),
                xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
                icons_dir: dirs.data_dir.join("icons"),
            },
            dirs,
        }
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
        let mut msg: Option<fn(&str)> = None;

        let err = operation
            .export_manifest(&output, &mut msg)
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
        let mut msg: Option<fn(&str)> = None;
        operation
            .export_manifest(&output, &mut msg)
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
