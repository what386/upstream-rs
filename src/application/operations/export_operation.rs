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
