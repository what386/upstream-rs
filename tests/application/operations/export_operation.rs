
use super::ExportOperation;
use crate::models::common::enums::{Channel, Filetype, Provider};
use crate::models::upstream::Package;
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
    std::env::temp_dir().join(format!("upstream-export-test-{name}-{nanos}"))
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
fn export_manifest_fails_when_no_installed_packages_exist() {
    let root = temp_root("empty");
    let paths = test_paths(&root);
    fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
        .expect("create metadata dir");
    let storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
    let operation = ExportOperation::new(&storage, &paths);
    let output = root.join("manifest.json");
    let mut msg: Option<fn(&str)> = None;

    let err = operation
        .export_manifest(&output, &mut msg)
        .expect_err("no installed packages");
    assert!(err.to_string().contains("No installed packages"));

    cleanup(&root).expect("cleanup");
}

#[test]
fn export_manifest_writes_installed_package_references() {
    let root = temp_root("manifest");
    let paths = test_paths(&root);
    fs::create_dir_all(paths.config.packages_file.parent().expect("parent"))
        .expect("create metadata dir");
    let mut storage = PackageStorage::new(&paths.config.packages_file).expect("storage");
    let mut pkg = Package::with_defaults(
        "tool".to_string(),
        "owner/tool".to_string(),
        Filetype::Binary,
        None,
        None,
        Channel::Stable,
        Provider::Github,
        None,
    );
    pkg.install_path = Some(paths.install.binaries_dir.join("tool"));
    storage
        .add_or_update_package(pkg)
        .expect("store installed package");

    let operation = ExportOperation::new(&storage, &paths);
    let output = root.join("manifest.json");
    let mut msg: Option<fn(&str)> = None;
    operation
        .export_manifest(&output, &mut msg)
        .expect("export manifest");

    let content = fs::read_to_string(&output).expect("read manifest");
    assert!(content.contains("\"version\": 1"));
    assert!(content.contains("\"name\": \"tool\""));
    assert!(content.contains("\"repo_slug\": \"owner/tool\""));

    cleanup(&root).expect("cleanup");
}
