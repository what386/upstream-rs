use super::PackageInstaller;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-package-installer-test-{name}-{nanos}"))
}

fn cleanup(path: &Path) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[test]
fn package_cache_key_sanitizes_disallowed_characters() {
    let key = PackageInstaller::package_cache_key("my/pkg v1.0");
    assert!(key.starts_with("my_pkg_v1_0-"));
    assert!(
        key.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    );
}

#[test]
fn find_macos_app_bundle_prefers_package_named_bundle() {
    let root = temp_root("app-bundle");
    fs::create_dir_all(root.join("Other.app")).expect("create other app");
    fs::create_dir_all(root.join("Tool.app")).expect("create package app");

    let bundle = PackageInstaller::find_macos_app_bundle(&root, "tool")
        .expect("find bundle")
        .expect("bundle");
    assert_eq!(
        bundle.file_name().and_then(|s| s.to_str()),
        Some("Tool.app")
    );

    cleanup(&root).expect("cleanup");
}

#[test]
fn find_macos_app_executable_reads_contents_macos() {
    let root = temp_root("app-exec");
    let macos_dir = root.join("Tool.app").join("Contents").join("MacOS");
    fs::create_dir_all(&macos_dir).expect("create macos dir");
    let exec = macos_dir.join("Tool");
    fs::write(&exec, b"#!/bin/sh\necho hi\n").expect("write executable");

    let found = PackageInstaller::find_macos_app_executable(&root.join("Tool.app"), "tool")
        .expect("find executable");
    assert_eq!(found, exec);

    cleanup(&root).expect("cleanup");
}
