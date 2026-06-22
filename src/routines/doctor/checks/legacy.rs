use std::{fs, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::models::upstream::Package;
use crate::utils::static_paths::UpstreamPaths;

pub(crate) const MIGRATE_DIR_HINT: &str =
    "Run `upstream doctor --migrate` to update local data for the current upstream layout.";
pub(crate) const LEGACY_PACKAGE_STORAGE_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
struct LegacyPackageStorageFile {
    version: u32,
    packages: Vec<Package>,
}

pub(crate) fn legacy_package_dirs(paths: &UpstreamPaths) -> [PathBuf; 3] {
    [
        paths.dirs.data_dir.join("appimages"),
        paths.dirs.data_dir.join("binaries"),
        paths.dirs.data_dir.join("archives"),
    ]
}

pub(crate) fn legacy_package_dirs_exist(paths: &UpstreamPaths) -> bool {
    legacy_package_dirs(paths).iter().any(|path| path.exists())
}

pub(crate) fn current_package_layout_incomplete(paths: &UpstreamPaths) -> bool {
    [
        paths.dirs.packages_dir.as_path(),
        paths.dirs.cache_dir.as_path(),
        paths.install.appimages_dir.as_path(),
        paths.install.binaries_dir.as_path(),
        paths.install.archives_dir.as_path(),
    ]
    .iter()
    .any(|path| !path.exists())
}

pub(crate) fn looks_like_legacy_layout(paths: &UpstreamPaths) -> bool {
    legacy_package_dirs_exist(paths) && current_package_layout_incomplete(paths)
}

pub(crate) fn previous_layout_version_hint(paths: &UpstreamPaths) -> Option<u32> {
    legacy_package_dirs_exist(paths).then_some(1)
}

pub(crate) fn legacy_package_metadata_exists(paths: &UpstreamPaths) -> bool {
    paths.config.packages_file.exists()
}

pub(crate) fn load_legacy_package_metadata(paths: &UpstreamPaths) -> Result<Vec<Package>> {
    let packages_file = &paths.config.packages_file;
    let json = fs::read_to_string(packages_file).with_context(|| {
        format!(
            "Failed to read package storage '{}'",
            packages_file.display()
        )
    })?;
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }

    let file: LegacyPackageStorageFile = serde_json::from_str(&json).with_context(|| {
        format!(
            "Failed to parse package storage '{}'. The file may be corrupt; restore from backup or fix JSON syntax",
            packages_file.display()
        )
    })?;
    if file.version != LEGACY_PACKAGE_STORAGE_VERSION {
        return Err(anyhow!(
            "Unsupported package storage version {} in '{}'. Expected version {}.",
            file.version,
            packages_file.display(),
            LEGACY_PACKAGE_STORAGE_VERSION
        ));
    }

    Ok(file.packages)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    use crate::utils::test_support;
    use crate::{
        models::common::enums::{Channel, Filetype, Provider},
        models::upstream::Package,
    };

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-legacy-check-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn test_package(name: &str) -> Package {
        Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        )
    }

    #[test]
    fn legacy_layout_detector_matches_old_package_dirs_with_missing_new_layout() {
        let root = temp_root("legacy-layout");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(paths.dirs.data_dir.join("binaries")).expect("create legacy binaries");

        assert!(super::legacy_package_dirs_exist(&paths));
        assert!(super::looks_like_legacy_layout(&paths));
        assert_eq!(super::previous_layout_version_hint(&paths), Some(1));

        fs::create_dir_all(&paths.dirs.packages_dir).expect("create packages");
        fs::create_dir_all(&paths.dirs.cache_dir).expect("create cache");
        fs::create_dir_all(&paths.install.appimages_dir).expect("create appimages");
        fs::create_dir_all(&paths.install.binaries_dir).expect("create binaries");
        fs::create_dir_all(&paths.install.archives_dir).expect("create archives");

        assert!(super::legacy_package_dirs_exist(&paths));
        assert!(!super::looks_like_legacy_layout(&paths));
        assert_eq!(super::previous_layout_version_hint(&paths), Some(1));

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn load_legacy_package_metadata_reads_versioned_json() {
        let root = temp_root("legacy-package-json");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create parent");
        fs::write(
            &paths.config.packages_file,
            serde_json::json!({
                "version": super::LEGACY_PACKAGE_STORAGE_VERSION,
                "packages": [test_package("tool")]
            })
            .to_string(),
        )
        .expect("write packages");

        let packages = super::load_legacy_package_metadata(&paths).expect("load packages");

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "tool");

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn load_legacy_package_metadata_rejects_unsupported_version() {
        let root = temp_root("legacy-package-json-bad-version");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create parent");
        fs::write(
            &paths.config.packages_file,
            r#"{"version":2,"packages":[]}"#,
        )
        .expect("write packages");

        let err = super::load_legacy_package_metadata(&paths).expect_err("version should fail");

        assert!(
            err.to_string()
                .contains("Unsupported package storage version")
        );

        cleanup(&root).expect("cleanup");
    }
}
