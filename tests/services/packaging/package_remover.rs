use super::PackageRemover;
use crate::models::common::enums::{Channel, Filetype, Provider};
use crate::models::upstream::Package;
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
    std::env::temp_dir().join(format!("upstream-remover-test-{name}-{nanos}"))
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
fn remove_path_if_exists_deletes_file_and_directory() {
    let root = temp_root("remove-path");
    let paths = test_paths(&root);
    fs::create_dir_all(&paths.integration.icons_dir).expect("create icons dir");
    let file = paths.integration.icons_dir.join("pkg.png");
    fs::write(&file, b"icon").expect("write icon");
    let nested_dir = root.join("to-remove");
    fs::create_dir_all(nested_dir.join("x")).expect("create nested dir");

    let remover = PackageRemover::new(&paths);
    let mut messages: Option<fn(&str)> = None;
    remover
        .remove_path_if_exists(&file, &mut messages)
        .expect("remove file");
    remover
        .remove_path_if_exists(&nested_dir, &mut messages)
        .expect("remove directory");
    remover
        .remove_path_if_exists(&root.join("missing"), &mut messages)
        .expect("ignore missing");

    assert!(!file.exists());
    assert!(!nested_dir.exists());

    cleanup(&root).expect("cleanup");
}

#[test]
fn remove_runtime_integrations_requires_install_path() {
    let root = temp_root("runtime-missing-path");
    let paths = test_paths(&root);
    fs::create_dir_all(&paths.config.paths_file.parent().expect("parent"))
        .expect("create metadata dir");
    fs::write(&paths.config.paths_file, "").expect("create paths file");

    let package = Package::with_defaults(
        "tool".to_string(),
        "owner/tool".to_string(),
        Filetype::Binary,
        None,
        None,
        Channel::Stable,
        Provider::Github,
        None,
    );
    let remover = PackageRemover::new(&paths);
    let mut messages: Option<fn(&str)> = None;

    let err = remover
        .remove_runtime_integrations(&package, &mut messages)
        .expect_err("must fail without install path");
    assert!(err.to_string().contains("no install path"));

    cleanup(&root).expect("cleanup");
}
