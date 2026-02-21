use crate::{
    application::operations::install_operation::InstallOperation,
    models::upstream::PackageReference,
    providers::provider_manager::ProviderManager,
    services::{
        packaging::{PackageInstaller, PackageRemover, PackageUpgrader},
        storage::package_storage::PackageStorage,
    },
    utils::static_paths::UpstreamPaths,
};
use anyhow::{Context, Result, anyhow};
use console::style;
use serde::Deserialize;
use std::{fs, path::Path};

// ---------------------------------------------------------------------------
// Manifest (mirrors ExportManifest but only needs Deserialize)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ImportManifest {
    pub version: u32,
    pub packages: Vec<PackageReference>,
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Returns true if the path looks like a tarball we produced.
fn is_snapshot(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name.ends_with(".tar.gz") || name.ends_with(".tgz")
}

// ---------------------------------------------------------------------------
// Operation
// ---------------------------------------------------------------------------

pub struct ImportOperation<'a> {
    provider_manager: &'a ProviderManager,
    package_storage: &'a mut PackageStorage,
    paths: &'a UpstreamPaths,
}

impl<'a> ImportOperation<'a> {
    pub fn new(
        provider_manager: &'a ProviderManager,
        package_storage: &'a mut PackageStorage,
        paths: &'a UpstreamPaths,
    ) -> Self {
        Self {
            provider_manager,
            package_storage,
            paths,
        }
    }

    /// Entry point — dispatches to manifest or snapshot based on the file.
    pub async fn import<F, G, H>(
        &mut self,
        path: &Path,
        skip_failed: bool,
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        if is_snapshot(path) {
            if skip_failed {
                message!(
                    message_callback,
                    "{}",
                    style("Note: --skip-failed has no effect for snapshot imports").yellow()
                );
            }
            self.import_snapshot(path, message_callback)
        } else {
            self.import_manifest(
                path,
                skip_failed,
                download_progress_callback,
                overall_progress_callback,
                message_callback,
            )
            .await
        }
    }

    // -----------------------------------------------------------------------
    // Light import (manifest)
    // -----------------------------------------------------------------------

    async fn import_manifest<F, G, H>(
        &mut self,
        path: &Path,
        skip_failed: bool,
        download_progress_callback: &mut Option<F>,
        overall_progress_callback: &mut Option<G>,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        F: FnMut(u64, u64),
        G: FnMut(u32, u32),
        H: FnMut(&str),
    {
        let content = fs::read_to_string(path)
            .context(format!("Failed to read manifest from '{}'", path.display()))?;

        let manifest: ImportManifest =
            serde_json::from_str(&content).context("Failed to parse manifest")?;

        if manifest.version != 1 {
            return Err(anyhow!(
                "Unsupported manifest version {}. Upgrade upstream and try again.",
                manifest.version
            ));
        }

        // Split into packages that need installing vs upgrading (force).
        let installed_names: std::collections::HashSet<&str> = self
            .package_storage
            .get_all_packages()
            .iter()
            .filter(|p| p.install_path.is_some())
            .map(|p| p.name.as_str())
            .collect();

        let mut to_install = Vec::new();
        let mut to_upgrade = Vec::new();
        let mut failures = 0_u32;

        for reference in &manifest.packages {
            if installed_names.contains(reference.name.as_str()) {
                to_upgrade.push(reference.clone());
            } else {
                to_install.push(reference.clone());
            }
        }

        // --- Upgrade already-installed packages (force) ---
        if !to_upgrade.is_empty() {
            message!(
                message_callback,
                "{}",
                style(format!(
                    "{} package(s) already installed — forcing upgrade",
                    to_upgrade.len()
                ))
                .yellow()
            );

            let installer = PackageInstaller::new(self.provider_manager, self.paths)?;
            let remover = PackageRemover::new(self.paths);
            let upgrader =
                PackageUpgrader::new(self.provider_manager, installer, remover, self.paths);

            for reference in &to_upgrade {
                let Some(package) = self
                    .package_storage
                    .get_package_by_name(&reference.name)
                    .cloned()
                else {
                    if skip_failed {
                        failures += 1;
                        message!(
                            message_callback,
                            "{} Package '{}' missing from storage; skipping",
                            style("Upgrade failed:").red(),
                            reference.name
                        );
                        continue;
                    }
                    return Err(anyhow!("Package '{}' not found in storage", reference.name));
                };

                message!(message_callback, "Upgrading '{}' ...", reference.name);

                match upgrader
                    .upgrade(&package, true, download_progress_callback, message_callback)
                    .await
                {
                    Ok(Some(updated)) => {
                        self.package_storage.add_or_update_package(updated)?;
                        message!(
                            message_callback,
                            "{}",
                            style(format!("'{}' upgraded", reference.name)).green()
                        );
                    }
                    Ok(None) => {
                        // Shouldn't happen with force=true, but harmless
                        message!(message_callback, "'{}' already up to date", reference.name);
                    }
                    Err(e) => {
                        if skip_failed {
                            failures += 1;
                            message!(message_callback, "{} {}", style("Upgrade failed:").red(), e);
                        } else {
                            return Err(e).context(format!(
                                "Failed to upgrade package '{}'",
                                reference.name
                            ));
                        }
                    }
                }
            }
        }

        // --- Install new packages via existing bulk path ---
        if !to_install.is_empty() {
            message!(
                message_callback,
                "Installing {} new package(s) ...",
                to_install.len()
            );

            let packages: Vec<_> = to_install.into_iter().map(|r| r.into_package()).collect();

            let mut install_op =
                InstallOperation::new(self.provider_manager, self.package_storage, self.paths)?;
            let total = packages.len() as u32;
            let mut completed = 0_u32;

            for package in packages {
                let package_name = package.name.clone();
                let use_icon = package.icon_path.is_some();
                message!(message_callback, "Installing '{}' ...", package_name);

                let install_result = install_op
                    .install_single(
                        package,
                        &None,
                        &use_icon,
                        download_progress_callback,
                        message_callback,
                    )
                    .await
                    .context(format!("Failed to install package '{}'", package_name));

                match install_result {
                    Ok(_) => {
                        message!(
                            message_callback,
                            "{}",
                            style(format!("'{}' installed", package_name)).green()
                        );
                    }
                    Err(err) => {
                        if skip_failed {
                            failures += 1;
                            message!(
                                message_callback,
                                "{} {}",
                                style("Install failed:").red(),
                                err
                            );
                        } else {
                            return Err(err);
                        }
                    }
                }

                completed += 1;
                if let Some(cb) = overall_progress_callback.as_mut() {
                    cb(completed, total);
                }
            }
        }

        if skip_failed && failures > 0 {
            message!(
                message_callback,
                "{} package(s) failed during import but were skipped",
                failures
            );
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Full import (snapshot)
    // -----------------------------------------------------------------------

    fn import_snapshot<H>(&mut self, path: &Path, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        let upstream_dir = &self.paths.dirs.data_dir;

        if upstream_dir.exists() {
            message!(
                message_callback,
                "{}",
                style("Warning: replacing existing upstream directory").yellow()
            );
            fs::remove_dir_all(upstream_dir).context(format!(
                "Failed to remove existing upstream directory '{}'",
                upstream_dir.display()
            ))?;
        }

        message!(message_callback, "Extracting snapshot ...");

        // Use a temp dir next to the target so rename is atomic (same filesystem).
        let temp_dir = upstream_dir
            .parent()
            .ok_or_else(|| anyhow!("upstream dir has no parent"))?
            .join(format!(".upstream-import-{}", std::process::id()));

        fs::create_dir_all(&temp_dir)?;

        // Decompress the tarball into temp_dir.  The archive contains an
        // "upstream/" top-level dir, so after extraction we move that into place.
        let extracted =
            crate::services::integration::compression_handler::decompress(path, &temp_dir)
                .context("Failed to extract snapshot")?;

        // The extracted path should be temp_dir/upstream (the top-level dir we
        // created during export).  Rename it directly to .upstream/.
        let source = if extracted.join("upstream").is_dir() {
            extracted.join("upstream")
        } else {
            // Flattened by decompress — the contents are already in extracted.
            extracted.clone()
        };

        fs::rename(&source, upstream_dir).context(format!(
            "Failed to move extracted snapshot to '{}'",
            upstream_dir.display()
        ))?;

        // Clean up temp dir (may already be gone if source == extracted).
        let _ = fs::remove_dir_all(&temp_dir);

        // Reload storage from the restored files.
        self.package_storage.load_packages().context(
            "Snapshot restored but failed to reload package storage — check the files manually",
        )?;

        message!(
            message_callback,
            "{}",
            style("Snapshot restored successfully").green()
        );

        Ok(())
    }
}

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}
use message;

#[cfg(test)]
#[path = "../../../tests/application/operations/import_operation.rs"]
mod tests;
