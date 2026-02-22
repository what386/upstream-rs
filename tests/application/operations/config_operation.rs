
use super::ConfigUpdater;
use crate::services::storage::config_storage::ConfigStorage;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_config_file(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("upstream-config-updater-test-{name}-{nanos}"))
        .join("config.toml")
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::remove_dir_all(parent)?;
    }
    Ok(())
}

#[test]
fn parse_set_key_requires_key_value_format() {
    assert!(ConfigUpdater::parse_set_key("github.rate_limit=10").is_ok());
    assert!(ConfigUpdater::parse_set_key("missing-separator").is_err());
    assert!(ConfigUpdater::parse_set_key("   =x").is_err());
}

#[test]
fn set_key_and_get_key_round_trip_value() {
    let config_file = temp_config_file("roundtrip");
    fs::create_dir_all(config_file.parent().expect("config parent")).expect("create parent");
    let mut storage = ConfigStorage::new(&config_file).expect("create storage");
    let mut updater = ConfigUpdater::new(&mut storage);
    let mut messages: Option<fn(&str)> = None;

    updater
        .set_key("github.rate_limit=123", &mut messages)
        .expect("set key");
    let value = updater
        .get_key("github.rate_limit", &mut messages)
        .expect("get key");
    assert_eq!(value, "123");

    cleanup(&config_file).expect("cleanup");
}

#[test]
fn set_bulk_continues_after_failures_and_applies_valid_keys() {
    let config_file = temp_config_file("bulk");
    fs::create_dir_all(config_file.parent().expect("config parent")).expect("create parent");
    let mut storage = ConfigStorage::new(&config_file).expect("create storage");
    let mut updater = ConfigUpdater::new(&mut storage);
    let mut messages: Option<fn(&str)> = None;
    let keys = vec![
        "github.rate_limit=321".to_string(),
        "badformat".to_string(),
        "gitlab.rate_limit=654".to_string(),
    ];

    updater
        .set_bulk(&keys, &mut messages)
        .expect("bulk set should not abort");
    let github = updater
        .get_key("github.rate_limit", &mut messages)
        .expect("github key");
    let gitlab = updater
        .get_key("gitlab.rate_limit", &mut messages)
        .expect("gitlab key");
    assert_eq!(github, "321");
    assert_eq!(gitlab, "654");

    cleanup(&config_file).expect("cleanup");
}
