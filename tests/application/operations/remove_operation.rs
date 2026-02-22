
use super::RemoveOperation;
use crate::services::storage::package_storage::PackageStorage;
use crate::utils::static_paths::{
    AppDirs, ConfigPaths, InstallPaths, IntegrationPaths, UpstreamPaths,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-remove-op-test-{name}-{nanos}"))
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
fn remove_single_returns_error_for_missing_package() {
    let root = temp_root("missing");
    let paths = test_paths(&root);
    fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
        .expect("create metadata dir");
    let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
    let mut op = RemoveOperation::new(&mut storage, &paths);
    let mut msg: Option<fn(&str)> = None;

    let err = op
        .remove_single("missing", &false, &mut msg)
        .expect_err("missing package");
    assert!(err.to_string().contains("is not installed"));

    cleanup(&root).expect("cleanup");
}

#[test]
fn remove_bulk_reports_failures_for_missing_packages() {
    let root = temp_root("bulk");
    let paths = test_paths(&root);
    fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
        .expect("create metadata dir");
    let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
    let mut op = RemoveOperation::new(&mut storage, &paths);
    let mut msg: Option<fn(&str)> = None;
    let mut progress_calls = Vec::new();
    let mut progress = Some(|done: u32, total: u32| {
        progress_calls.push((done, total));
    });
    let names = vec!["a".to_string(), "b".to_string()];

    let (removed, failed) = op
        .remove_bulk(&names, &false, &mut msg, &mut progress)
        .expect("bulk remove");
    assert_eq!((removed, failed), (0, 2));
    assert_eq!(progress_calls.last().copied(), Some((2, 2)));

    cleanup(&root).expect("cleanup");
}
