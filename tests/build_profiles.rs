// These tests intentionally run real external build toolchains. They are gated to Linux because
// cargo xwin executes the test binary under Wine, where the host's Cargo, Go, Zig, CMake, and
// .NET installations are not available.
#![cfg(target_os = "linux")]

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use upstream_rs::routines::build::profiles::{
    BuildProfileHandler, cmake::CmakeProfile, dotnet::DotnetProfile, go::GoProfile,
    rust::RustProfile, zig::ZigProfile,
};
use walkdir::WalkDir;

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/repositories")
        .join(name)
}

fn copy_fixture(name: &str) -> (PathBuf, impl Drop) {
    let destination = std::env::temp_dir().join(format!(
        "upstream-build-fixture-{}-{}",
        name,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));

    let source = fixture_path(name);

    for entry in WalkDir::new(&source) {
        let entry = entry.expect("walk fixture");
        let relative = entry.path().strip_prefix(&source).expect("fixture prefix");
        let path = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(path).expect("create fixture directory");
        } else {
            fs::create_dir_all(path.parent().expect("fixture parent"))
                .expect("create fixture parent");
            fs::copy(entry.path(), path).expect("copy fixture file");
        }
    }

    struct Cleanup(PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    (destination.clone(), Cleanup(destination))
}

fn build_fixture<P: BuildProfileHandler>(
    profile: P,
    name: &str,
    target_hint: &str,
    expected_artifact: &str,
) {
    let (workspace, _cleanup) = copy_fixture(name);
    let mut callback = None;

    assert!(profile.detect(&workspace));
    let artifact = profile
        .run_build(&workspace, target_hint, &mut callback)
        .expect("build fixture repository");

    assert_eq!(
        artifact.file_name().and_then(|name| name.to_str()),
        Some(expected_artifact)
    );

    assert!(
        artifact.is_file(),
        "missing artifact at {}",
        artifact.display()
    );
}

#[test]
fn builds_a_real_single_package_repository() {
    build_fixture(RustProfile, "rust", "friendly-single", "single-fixture");
}

#[test]
fn builds_a_real_virtual_workspace_member() {
    build_fixture(RustProfile, "rust-workspace", "app", "workspace-fixture");
}

#[test]
fn builds_a_real_go_repository() {
    build_fixture(GoProfile, "go", "friendly-go", "fixture-go");
}

#[test]
fn builds_a_real_dotnet_repository() {
    build_fixture(
        DotnetProfile,
        "dotnet-sln",
        "friendly-dotnet",
        "dotnet-fixture",
    );
}

#[test]
fn builds_a_real_dotnet_slnx_repository() {
    build_fixture(
        DotnetProfile,
        "dotnet-slnx",
        "friendly-dotnet",
        "dotnet-fixture",
    );
}

#[test]
fn builds_a_real_zig_repository() {
    build_fixture(ZigProfile, "zig", "friendly-zig", "zig-fixture");
}

#[test]
fn builds_a_real_cmake_repository() {
    build_fixture(CmakeProfile, "cmake", "friendly-cmake", "cmake-fixture");
}
