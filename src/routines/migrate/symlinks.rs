use anyhow::{Context, Result};

use crate::models::upstream::Package;
use crate::routines::migrate::MigrationReport;
use crate::services::integration::SymlinkManager;
use crate::utils::static_paths::UpstreamPaths;

pub(in crate::routines::migrate) fn refresh_symlinks(
    paths: &UpstreamPaths,
    packages: &[Package],
    report: &mut MigrationReport,
) -> Result<()> {
    let symlink_manager = SymlinkManager::new(&paths.integration.symlinks_dir);

    for package in packages {
        let target = package.exec_path.as_ref().or(package.install_path.as_ref());
        let Some(target) = target else {
            report.skipped_symlinks += 1;
            continue;
        };
        if !target.exists() {
            report.skipped_symlinks += 1;
            continue;
        }

        symlink_manager
            .add_link(target, &package.name)
            .with_context(|| format!("Failed to refresh symlink for '{}'", package.name))?;
        report.refreshed_symlinks += 1;
    }

    Ok(())
}
