    use super::{ImportOperation, is_snapshot};
    use crate::providers::provider_manager::ProviderManager;
    use crate::services::storage::package_storage::PackageStorage;
    use crate::utils::static_paths::{AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("upstream-import-test-{name}-{nanos}"))
    }

    fn test_paths(root: &PathBuf) -> UpstreamPaths {
        let dirs = AppDirs {
            user_dir: root.clone(),
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            metadata_dir: root.join("data/metadata"),
        };

        UpstreamPaths {
            config: ConfigPaths {
                config_file: dirs.config_dir.join("config.toml"),
                packages_file: dirs.metadata_dir.join("packages.json"),
                paths_file: dirs.metadata_dir.join("paths.sh"),
            },
            install: InstallPaths {
                appimages_dir: dirs.data_dir.join("appimages"),
                binaries_dir: dirs.data_dir.join("binaries"),
                archives_dir: dirs.data_dir.join("archives"),
            },
            integration: IntegrationPaths {
                symlinks_dir: dirs.data_dir.join("symlinks"),
                xdg_applications_dir: dirs.user_dir.join(".local/share/applications"),
                icons_dir: dirs.data_dir.join("icons"),
            },
            dirs,
        }
    }

    fn cleanup(path: &PathBuf) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[test]
    fn snapshot_detection_matches_supported_extensions() {
        assert!(is_snapshot(std::path::Path::new("backup.tar.gz")));
        assert!(is_snapshot(std::path::Path::new("backup.tgz")));
        assert!(!is_snapshot(std::path::Path::new("manifest.json")));
    }

    #[tokio::test]
    async fn import_manifest_rejects_unsupported_manifest_version() {
        let root = temp_root("bad-version");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let manifest_path = root.join("manifest.json");
        fs::write(
            &manifest_path,
            r#"{"version":2,"packages":[{"name":"x","repo_slug":"o/r","filetype":"Binary","channel":"Stable","provider":"Github","base_url":null,"match_pattern":null,"exclude_pattern":null}]}"#,
        )
        .expect("write manifest");

        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let manager = ProviderManager::new(None, None, None, None).expect("provider manager");
        let mut operation = ImportOperation::new(&manager, &mut storage, &paths);
        let mut dlp: Option<fn(u64, u64)> = None;
        let mut op: Option<fn(u32, u32)> = None;
        let mut msg: Option<fn(&str)> = None;

        let err = operation
            .import(&manifest_path, false, &mut dlp, &mut op, &mut msg)
            .await
            .expect_err("must reject unsupported version");
        assert!(err.to_string().contains("Unsupported manifest version"));

        cleanup(&root).expect("cleanup");
    }
