    use super::InstallOperation;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;
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
        std::env::temp_dir().join(format!("upstream-install-op-test-{name}-{nanos}"))
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

    #[tokio::test]
    async fn perform_install_rejects_already_installed_package_before_network_calls() {
        let root = temp_root("already-installed");
        let paths = test_paths(&root);
        fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
            .expect("create metadata dir");
        let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
        let provider_manager = ProviderManager::new(None, None, None, None).expect("provider manager");
        let op = InstallOperation::new(&provider_manager, &mut storage, &paths).expect("operation");

        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Binary,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            None,
        );
        package.install_path = Some(paths.install.binaries_dir.join("tool"));
        let mut dl: Option<fn(u64, u64)> = None;
        let mut msg: Option<fn(&str)> = None;

        let err = op
            .perform_install(package, &None, &mut dl, &mut msg)
            .await
            .expect_err("already-installed guard");
        assert!(err.to_string().contains("already installed"));

        cleanup(&root).expect("cleanup");
    }
