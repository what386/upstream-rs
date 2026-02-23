use super::ConfigStorage;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_config_file(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("upstream-config-test-{name}-{nanos}"))
        .join("config.toml")
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::remove_dir_all(parent)?;
    }
    Ok(())
}

#[test]
fn new_creates_default_config_file_when_missing() {
    let path = temp_config_file("new-default");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }

    let storage = ConfigStorage::new(&path).expect("create storage");
    assert!(path.exists());
    assert_eq!(storage.get_config().github.rate_limit, 5000);
    assert_eq!(storage.get_config().gitlab.rate_limit, 5000);

    cleanup(&path).expect("cleanup");
}

#[test]
fn set_and_get_nested_values_updates_config() {
    let path = temp_config_file("set-get");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    let mut storage = ConfigStorage::new(&path).expect("create storage");

    storage
        .try_set_value("github.rate_limit", "1234")
        .expect("set integer");
    storage
        .try_set_value("gitlab.api_token", "\"abc\"")
        .expect("set string literal");

    let rate_limit: u32 = storage
        .try_get_value("github.rate_limit")
        .expect("read rate limit");
    let token: Option<String> = storage
        .try_get_value("gitlab.api_token")
        .expect("read token");

    assert_eq!(rate_limit, 1234);
    assert_eq!(token.as_deref(), Some("abc"));

    cleanup(&path).expect("cleanup");
}

#[test]
fn flattened_config_contains_dot_notation_keys() {
    let path = temp_config_file("flatten");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    let storage = ConfigStorage::new(&path).expect("create storage");
    let flat = storage.get_flattened_config();

    assert_eq!(flat.get("github.rate_limit"), Some(&"5000".to_string()));
    assert_eq!(flat.get("gitlab.rate_limit"), Some(&"5000".to_string()));

    cleanup(&path).expect("cleanup");
}

#[test]
fn set_value_rejects_unknown_paths() {
    let path = temp_config_file("bad-path");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    let mut storage = ConfigStorage::new(&path).expect("create storage");
    let err = storage
        .try_set_value("github.missing.field", "1")
        .expect_err("must reject unknown path");
    assert!(err.contains("Key path not found"));

    cleanup(&path).expect("cleanup");
}

#[test]
fn reset_to_defaults_restores_default_values() {
    let path = temp_config_file("reset");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    let mut storage = ConfigStorage::new(&path).expect("create storage");
    storage
        .try_set_value("github.rate_limit", "99")
        .expect("set override");
    storage.reset_to_defaults().expect("reset defaults");

    let rate_limit: u32 = storage
        .try_get_value("github.rate_limit")
        .expect("read reset value");
    assert_eq!(rate_limit, 5000);

    cleanup(&path).expect("cleanup");
}
