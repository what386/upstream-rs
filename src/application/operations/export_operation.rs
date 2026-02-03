use crate::{
    models::upstream::PackageReference,
    services::storage::package_storage::PackageStorage,
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;
use std::{fs, path::Path};
use tar::Builder;

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
    pub fn export_manifest(&self, output: &Path) -> Result<()> {
        let packages = self.package_storage.get_all_packages();

        let references: Vec<PackageReference> = packages
            .iter()
            .filter(|p| p.install_path.is_some())
            .map(|p| PackageReference::from_package(p.clone()))
            .collect();

        if references.is_empty() {
            return Err(anyhow!("No installed packages to export"));
        }

        let manifest = ExportManifest {
            version: 1,
            exported_at: chrono::Utc::now().to_rfc3339(),
            packages: references,
        };

        let json = serde_json::to_string_pretty(&manifest)
            .context("Failed to serialise manifest")?;

        fs::write(output, json)
            .context(format!("Failed to write manifest to '{}'", output.display()))
    }

    /// Full export: tarball the entire .upstream directory.
    pub fn export_snapshot(&self, output: &Path) -> Result<()> {
        let upstream_dir = &self.paths.dirs.data_dir;

        if !upstream_dir.exists() {
            return Err(anyhow!(
                "Upstream directory '{}' does not exist — nothing to snapshot",
                upstream_dir.display()
            ));
        }

        let file = fs::File::create(output).context(format!(
            "Failed to create archive at '{}'",
            output.display()
        ))?;

        let gz = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(gz);

        // Archive the contents under a top-level "upstream/" directory so
        // extraction is unambiguous regardless of where the user extracts it.
        tar.append_dir_all("upstream", upstream_dir)
            .context(format!(
                "Failed to archive '{}'",
                upstream_dir.display()
            ))?;

        tar.finish()
            .context("Failed to finalise snapshot archive")
    }
}
