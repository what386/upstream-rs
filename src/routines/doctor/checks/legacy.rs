use std::path::PathBuf;

use crate::utils::static_paths::UpstreamPaths;

pub(crate) const MIGRATE_DIR_HINT: &str =
    "Run `upstream migrate` to update local data for the current upstream layout.";

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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    use crate::utils::test_support;

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-legacy-check-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
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
}
