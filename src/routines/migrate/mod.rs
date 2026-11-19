use anyhow::{Result, bail};

use crate::{
    storage::{database::PackageDatabase, manifest::ManifestStorage},
    utils::static_paths::UpstreamPaths,
};

pub fn run(paths: &UpstreamPaths) -> Result<()> {
    let mut ledger = ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))?;
    if ledger.manifest().is_some() {
        return Ok(());
    }

    for path in [
        paths.dirs.data_dir.join("appimages"),
        paths.dirs.data_dir.join("binaries"),
        paths.dirs.data_dir.join("archives"),
        paths.dirs.data_dir.join("symlinks"),
        paths.dirs.data_dir.join("icons"),
        paths.dirs.metadata_dir.join("paths.sh"),
        paths.dirs.metadata_dir.join("paths.nu"),
        paths.metadata.packages_file.clone(),
    ] {
        if path.try_exists()? {
            bail!(
                "Unsupported legacy installation found at '{}'. Upgrade this installation with Upstream v2.24.1 before using this version.",
                path.display()
            );
        }
    }
    // Do not certify an installation whose database cannot be upgraded safely.
    if paths.metadata.packages_database_file.try_exists()? {
        PackageDatabase::open(&paths.metadata.packages_database_file)?;
    }
    ledger.ensure_current()
}
