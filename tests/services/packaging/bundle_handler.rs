use super::BundleHandler;
use crate::utils::static_paths::UpstreamPaths;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-macbundle-test-{name}-{nanos}"))
}

fn cleanup(path: &Path) -> io::Result<()> {
    fs::remove_dir_all(path)
}

fn write_sized_file(path: &Path, size: usize) {
    fs::write(path, vec![0u8; size]).expect("write sized file");
}

#[test]
fn find_macos_app_bundle_prefers_package_named_bundle() {
    let root = temp_root("app-bundle");
    fs::create_dir_all(root.join("Other.app")).expect("create other app");
    fs::create_dir_all(root.join("Tool.app")).expect("create package app");

    let bundle = BundleHandler::find_macos_app_bundle(&root, "tool")
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

    let found = BundleHandler::find_macos_app_executable(&root.join("Tool.app"), "tool")
        .expect("find executable");
    assert_eq!(found, exec);

    cleanup(&root).expect("cleanup");
}

#[test]
fn select_macos_app_bundle_prefers_name_match_over_size() {
    let root = temp_root("select-name");
    let matched = root.join("Tool.app");
    let larger = root.join("Other.app");
    fs::create_dir_all(&matched).expect("create matched app");
    fs::create_dir_all(&larger).expect("create larger app");
    write_sized_file(&matched.join("small"), 16);
    write_sized_file(&larger.join("large"), 4096);

    let selected =
        BundleHandler::select_macos_app_bundle(&[larger.clone(), matched.clone()], "tool")
            .expect("select app bundle");
    assert_eq!(selected, matched);

    cleanup(&root).expect("cleanup");
}

#[test]
fn select_macos_app_bundle_falls_back_to_largest_when_no_name_match() {
    let root = temp_root("select-largest");
    let small = root.join("Alpha.app");
    let large = root.join("Beta.app");
    fs::create_dir_all(&small).expect("create small app");
    fs::create_dir_all(&large).expect("create large app");
    write_sized_file(&small.join("small"), 64);
    write_sized_file(&large.join("large"), 4096);

    let selected = BundleHandler::select_macos_app_bundle(&[small.clone(), large.clone()], "tool")
        .expect("select app bundle");
    assert_eq!(selected, large);

    cleanup(&root).expect("cleanup");
}

#[test]
fn find_macos_app_bundles_ignores_nested_bundle_entries() {
    let root = temp_root("find-bundles");
    let top = root.join("Tool.app");
    let nested = top.join("Contents").join("Resources").join("Nested.app");
    fs::create_dir_all(&nested).expect("create nested app bundle");

    let bundles = BundleHandler::find_macos_app_bundles(&root).expect("find app bundles");
    assert_eq!(bundles, vec![top]);

    cleanup(&root).expect("cleanup");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn install_dmg_errors_on_non_macos_hosts() {
    let root = temp_root("dmg-non-macos");
    fs::create_dir_all(&root).expect("create root");
    let dmg_path = root.join("app.dmg");
    fs::write(&dmg_path, b"not-a-real-dmg").expect("write dmg");

    let paths = UpstreamPaths::new();
    let handler = BundleHandler::new(&paths, &root);
    let package = crate::models::upstream::Package::with_defaults(
        "tool".to_string(),
        "owner/tool".to_string(),
        crate::models::common::enums::Filetype::MacDmg,
        None,
        None,
        crate::models::common::enums::Channel::Stable,
        crate::models::common::enums::Provider::Github,
        None,
    );
    let mut message_callback: Option<fn(&str)> = None;

    let err = handler
        .install_dmg(&dmg_path, package, &mut message_callback)
        .expect_err("non-macos should reject dmg install");
    assert!(
        err.to_string()
            .contains("DMG installation is only supported on macOS hosts")
    );

    cleanup(&root).expect("cleanup");
}
