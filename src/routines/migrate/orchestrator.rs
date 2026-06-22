use anyhow::Result;

use crate::{
    routines::doctor::checks::legacy,
    storage::manifest_storage::{CURRENT_LAYOUT_VERSION, ManifestStorage},
    utils::static_paths::UpstreamPaths,
};

use super::{layout, metadata, symlinks, trust};
use crate::routines::migrate::MigrationReport;

pub fn run(paths: &UpstreamPaths) -> Result<MigrationReport> {
    let rewrites = layout::package_path_rewrites(paths);
    let mut manifest_storage =
        ManifestStorage::new(&ManifestStorage::path_for_root(&paths.dirs.data_dir))?;
    let previous_layout_version = manifest_storage
        .manifest()
        .map(|manifest| manifest.layout_version)
        .or_else(|| legacy::previous_layout_version_hint(paths));
    let mut report = MigrationReport::default();

    layout::create_required_dirs(paths, &mut report)?;
    layout::move_legacy_package_dirs(&rewrites, &mut report)?;
    let packages = metadata::migrate_package_metadata(paths, &rewrites, &mut report)?;
    metadata::migrate_rollback_metadata(paths, &rewrites, &mut report)?;
    trust::migrate_trust_config(paths, &mut report)?;
    symlinks::refresh_symlinks(paths, &packages, &mut report)?;
    manifest_storage.record_migration_from(previous_layout_version, CURRENT_LAYOUT_VERSION)?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
    use crate::storage::manifest_storage::{
        CURRENT_LAYOUT_VERSION, MANIFEST_STORAGE_VERSION, ManifestStorage,
    };
    use crate::storage::rollback_storage::{
        RollbackArtifactFormat, RollbackRecord, RollbackSource,
    };
    use crate::utils::test_support;
    use chrono::Utc;
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        test_support::temp_root("upstream-migrate-test", name)
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn test_package(name: &str, install_path: PathBuf, exec_path: PathBuf) -> Package {
        let mut package = Package::with_defaults(
            name.to_string(),
            format!("owner/{name}"),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(install_path);
        package.exec_path = Some(exec_path);
        package
    }

    #[test]
    fn migrate_moves_package_dirs_and_rewrites_metadata() {
        let root = temp_root("layout");
        let paths = test_support::upstream_paths(&root);
        let old_binary = paths.dirs.data_dir.join("binaries").join("tool");
        let new_binary = paths.dirs.packages_dir.join("binaries").join("tool");
        fs::create_dir_all(old_binary.parent().expect("old binary parent"))
            .expect("create old binary parent");
        fs::write(&old_binary, b"tool").expect("write old binary");

        #[cfg(unix)]
        {
            fs::create_dir_all(&paths.integration.symlinks_dir).expect("create symlinks");
            std::os::unix::fs::symlink(&old_binary, paths.integration.symlinks_dir.join("tool"))
                .expect("create old symlink");
        }

        let package = test_package("tool", old_binary.clone(), old_binary.clone());
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");
        fs::write(
            &paths.config.packages_file,
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "packages": [package],
            }))
            .expect("serialize packages"),
        )
        .expect("write packages");

        let report = run(&paths).expect("migrate");

        assert!(!old_binary.exists());
        assert_eq!(
            fs::read(&new_binary).expect("read migrated binary"),
            b"tool"
        );
        assert_eq!(report.updated_packages, 1);
        assert_eq!(report.refreshed_symlinks, 1);

        let migrated: serde_json::Value = serde_json::from_slice(
            &fs::read(&paths.config.packages_file).expect("read migrated packages"),
        )
        .expect("parse migrated packages");
        assert_eq!(
            migrated["packages"][0]["install_path"].as_str(),
            Some(new_binary.to_str().expect("utf8 path"))
        );
        assert_eq!(
            migrated["packages"][0]["exec_path"].as_str(),
            Some(new_binary.to_str().expect("utf8 path"))
        );
        let migration_manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(ManifestStorage::path_for_root(&paths.dirs.data_dir))
                .expect("read migration manifest"),
        )
        .expect("parse migration manifest");
        assert_eq!(
            migration_manifest["manifest_version"].as_u64(),
            Some(MANIFEST_STORAGE_VERSION as u64)
        );
        assert_eq!(
            migration_manifest["layout_version"].as_u64(),
            Some(CURRENT_LAYOUT_VERSION as u64)
        );
        assert_eq!(
            migration_manifest["previous_layout_version"].as_u64(),
            Some(1)
        );

        #[cfg(unix)]
        assert_eq!(
            fs::read_link(paths.integration.symlinks_dir.join("tool")).expect("read symlink"),
            new_binary
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_rewrites_rollback_package_snapshots() {
        let root = temp_root("rollback");
        let paths = test_support::upstream_paths(&root);
        let old_archive = paths
            .dirs
            .data_dir
            .join("archives")
            .join("tool")
            .join("bin")
            .join("tool");
        let new_archive = paths
            .dirs
            .packages_dir
            .join("archives")
            .join("tool")
            .join("bin")
            .join("tool");
        fs::create_dir_all(old_archive.parent().expect("old archive parent"))
            .expect("create old archive parent");
        fs::write(&old_archive, b"tool").expect("write old archive executable");
        fs::create_dir_all(&paths.dirs.metadata_dir).expect("create metadata");

        let package = test_package(
            "tool",
            paths.dirs.data_dir.join("archives").join("tool"),
            old_archive.clone(),
        );
        let record = RollbackRecord {
            package_snapshot: package,
            artifact_relative_path: PathBuf::from("tool/archive.tgz"),
            icon_relative_path: None,
            artifact_format: RollbackArtifactFormat::Tgz,
            artifact_entry_path: Some(PathBuf::from("artifact/tool")),
            icon_entry_path: None,
            source: RollbackSource::Upgrade,
            created_at: Utc::now(),
        };
        fs::write(
            paths.dirs.metadata_dir.join("rollback.json"),
            serde_json::to_vec_pretty(&json!({
                "version": 1,
                "records": {
                    "tool": [record],
                },
            }))
            .expect("serialize rollback"),
        )
        .expect("write rollback");

        let report = run(&paths).expect("migrate");

        assert_eq!(
            fs::read(&new_archive).expect("read migrated archive"),
            b"tool"
        );
        assert_eq!(report.updated_rollback_records, 1);
        let migrated: serde_json::Value = serde_json::from_slice(
            &fs::read(paths.dirs.metadata_dir.join("rollback.json")).expect("read rollback"),
        )
        .expect("parse rollback");
        assert_eq!(
            migrated["records"]["tool"][0]["package_snapshot"]["install_path"].as_str(),
            Some(
                paths
                    .dirs
                    .packages_dir
                    .join("archives")
                    .join("tool")
                    .to_str()
                    .expect("utf8 path")
            )
        );
        assert_eq!(
            migrated["records"]["tool"][0]["package_snapshot"]["exec_path"].as_str(),
            Some(new_archive.to_str().expect("utf8 path"))
        );

        cleanup(&root).expect("cleanup");
    }

    #[test]
    fn migrate_moves_legacy_config_trust_keys_to_trust_storage() {
        let root = temp_root("trust-config");
        let paths = test_support::upstream_paths(&root);
        fs::create_dir_all(&paths.dirs.config_dir).expect("create config");
        fs::write(
            &paths.config.config_file,
            r#"
[github]
api_token = "ghp_abc"

[trust]
minisign_public_keys = [{ id = "mini", key = "RWabc" }]
cosign_public_keys = [{ id = "cosign", key = "-----BEGIN PUBLIC KEY-----\nkey\n-----END PUBLIC KEY-----" }]
"#,
        )
        .expect("write legacy config");

        let report = run(&paths).expect("migrate");

        assert_eq!(report.migrated_trusted_keys, 2);
        let migrated_config =
            fs::read_to_string(&paths.config.config_file).expect("read migrated config");
        assert!(migrated_config.contains("version = 2"));
        assert!(!migrated_config.contains("[trust]"));

        let trust_json: serde_json::Value = serde_json::from_slice(
            &fs::read(&paths.config.trust_file).expect("read trust storage"),
        )
        .expect("parse trust storage");
        assert_eq!(
            trust_json["minisign_public_keys"][0]["id"].as_str(),
            Some("mini")
        );
        assert_eq!(
            trust_json["cosign_public_keys"][0]["id"].as_str(),
            Some("cosign")
        );

        cleanup(&root).expect("cleanup");
    }
}
