
use super::MetadataManager;
use crate::models::common::enums::{Channel, Filetype, Provider};
use crate::models::upstream::Package;
use crate::services::storage::package_storage::PackageStorage;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_packages_file(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("upstream-metadata-test-{name}-{nanos}"))
        .join("packages.json")
}

fn test_package(name: &str) -> Package {
    Package::with_defaults(
        name.to_string(),
        format!("owner/{name}"),
        Filetype::Archive,
        None,
        None,
        Channel::Stable,
        Provider::Github,
        None,
    )
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::remove_dir_all(parent)?;
    }
    Ok(())
}

#[test]
fn parse_set_key_requires_key_value_pair() {
    assert!(MetadataManager::parse_set_key("is_pinned=true").is_ok());
    assert!(MetadataManager::parse_set_key("invalid").is_err());
    assert!(MetadataManager::parse_set_key("=value").is_err());
}

#[test]
fn pin_and_unpin_update_package_state() {
    let path = temp_packages_file("pin");
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let mut storage = PackageStorage::new(&path).expect("create storage");
    storage
        .add_or_update_package(test_package("fd"))
        .expect("store package");
    let mut manager = MetadataManager::new(&mut storage);
    let mut messages: Option<fn(&str)> = None;

    manager
        .pin_package("fd", &mut messages)
        .expect("pin package");
    assert!(
        manager
            .package_storage
            .get_package_by_name("fd")
            .expect("package")
            .is_pinned
    );

    manager
        .unpin_package("fd", &mut messages)
        .expect("unpin package");
    assert!(
        !manager
            .package_storage
            .get_package_by_name("fd")
            .expect("package")
            .is_pinned
    );

    cleanup(&path).expect("cleanup");
}

#[test]
fn set_key_and_get_key_support_nested_and_typed_values() {
    let path = temp_packages_file("set-get");
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let mut storage = PackageStorage::new(&path).expect("create storage");
    storage
        .add_or_update_package(test_package("rg"))
        .expect("store package");
    let mut manager = MetadataManager::new(&mut storage);
    let mut messages: Option<fn(&str)> = None;

    manager
        .set_key("rg", "is_pinned=true", &mut messages)
        .expect("set bool key");
    manager
        .set_key("rg", "version.major=12", &mut messages)
        .expect("set nested numeric key");

    assert_eq!(
        manager
            .get_key("rg", "is_pinned", &mut messages)
            .expect("get bool"),
        "true"
    );
    assert_eq!(
        manager
            .get_key("rg", "version.major", &mut messages)
            .expect("get nested"),
        "12"
    );

    cleanup(&path).expect("cleanup");
}

#[test]
fn rename_package_rejects_duplicates_and_updates_alias() {
    let path = temp_packages_file("rename");
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let mut storage = PackageStorage::new(&path).expect("create storage");
    storage
        .add_or_update_package(test_package("old"))
        .expect("store old");
    storage
        .add_or_update_package(test_package("taken"))
        .expect("store taken");
    let mut manager = MetadataManager::new(&mut storage);
    let mut messages: Option<fn(&str)> = None;

    assert!(
        manager
            .rename_package("old", "taken", &mut messages)
            .is_err()
    );
    manager
        .rename_package("old", "new", &mut messages)
        .expect("rename package");
    assert!(manager.package_storage.get_package_by_name("new").is_some());
    assert!(manager.package_storage.get_package_by_name("old").is_none());

    cleanup(&path).expect("cleanup");
}
