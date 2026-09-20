use std::fs;

use anyhow::{Context, Result};

use crate::{
    routines::migrate::{MigrationReport, step::Step},
    storage::manifest::{CURRENT_LAYOUT_VERSION, ManifestStorage},
    utils::static_paths::UpstreamPaths,
};

pub struct V2_20_0;

pub(super) fn run(paths: &UpstreamPaths, report: &mut MigrationReport) -> Result<()> {
    V2_20_0::run(paths, report)
}

impl Step for V2_20_0 {
    fn check(paths: &UpstreamPaths) -> Result<bool> {
        let manifest = ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))?;
        Ok(manifest
            .manifest()
            .is_none_or(|manifest| manifest.layout_version < CURRENT_LAYOUT_VERSION))
    }

    fn apply(paths: &UpstreamPaths, _report: &mut MigrationReport) -> Result<()> {
        for path in [
            paths.dirs.metadata_dir.join("rollback.json"),
            paths.dirs.state_dir.join("rollback"),
            paths.dirs.data_dir.join("rollback"),
        ] {
            if !path.exists() {
                continue;
            }

            if path.is_dir() {
                fs::remove_dir_all(&path).with_context(|| {
                    format!(
                        "Failed to remove obsolete rollback storage '{}'",
                        path.display()
                    )
                })?;
            } else {
                fs::remove_file(&path).with_context(|| {
                    format!(
                        "Failed to remove obsolete rollback metadata '{}'",
                        path.display()
                    )
                })?;
            }
        }

        let mut manifest =
            ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))?;

        manifest.record_migration(CURRENT_LAYOUT_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::{V2_20_0, run};
    use crate::{
        routines::migrate::{MigrationReport, step::Step},
        storage::manifest::{CURRENT_LAYOUT_VERSION, ManifestStorage},
        utils::test_support,
    };
    use std::fs;

    #[test]
    fn migration_removes_persistent_rollback_data() {
        let root = test_support::temp_root("upstream-migrate-v2-20-test", "rollback-removal");
        let paths = test_support::upstream_paths(&root);
        let metadata = paths.dirs.metadata_dir.join("rollback.json");
        let state = paths.dirs.state_dir.join("rollback");
        let legacy = paths.dirs.data_dir.join("rollback");

        fs::create_dir_all(&state).expect("create state rollback directory");
        fs::create_dir_all(&legacy).expect("create legacy rollback directory");
        fs::create_dir_all(metadata.parent().expect("metadata parent")).expect("create metadata");
        fs::write(&metadata, "{}").expect("write rollback metadata");

        let mut report = MigrationReport::default();
        run(&paths, &mut report).expect("run rollback removal migration");

        assert!(!metadata.exists());
        assert!(!state.exists());
        assert!(!legacy.exists());
        assert!(!V2_20_0::check(&paths).expect("check migration"));

        let manifest = ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))
            .expect("open migration manifest");

        assert_eq!(
            manifest.manifest().expect("manifest").layout_version,
            CURRENT_LAYOUT_VERSION
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
