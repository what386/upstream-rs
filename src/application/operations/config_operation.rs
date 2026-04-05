use crate::services::storage::config_storage::ConfigStorage;
use anyhow::Result;
use console::style;
use toml;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct ConfigUpdater<'a> {
    config_storage: &'a mut ConfigStorage,
}

impl<'a> ConfigUpdater<'a> {
    pub fn new(config_storage: &'a mut ConfigStorage) -> Self {
        Self { config_storage }
    }

    /// Sets a configuration value using dot-notation key path.
    /// Example: "parent.child=value" or "github.api_token=abc123"
    pub fn set_key<H>(&mut self, set_key: &str, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        let (key_path, value) = Self::parse_set_key(set_key)?;

        message!(message_callback, "Setting '{}' = '{}'", key_path, value);

        self.config_storage.try_set_value(&key_path, &value)?;

        message!(
            message_callback,
            "{}",
            style("Configuration updated successfully").green()
        );

        Ok(())
    }

    /// Gets a configuration value using dot-notation key path.
    /// Example: "parent.child" or "github.api_token"
    pub fn get_key<H>(&self, get_key: &str, message_callback: &mut Option<H>) -> Result<String>
    where
        H: FnMut(&str),
    {
        let key_path = get_key.trim();

        if key_path.is_empty() {
            return Err(anyhow::anyhow!("Key path cannot be empty"));
        }

        message!(message_callback, "Getting value for '{}'", key_path);

        let value: toml::Value = self.config_storage.try_get_value(key_path)?;

        let value_str = Self::format_value(&value);

        message!(
            message_callback,
            "{} = {}",
            key_path,
            style(&value_str).cyan()
        );

        Ok(value_str)
    }

    /// Sets multiple configuration values in bulk.
    pub fn set_bulk<H>(
        &mut self,
        set_keys: &[String],
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let mut failures = 0;

        for set_key in set_keys {
            match self.set_key(set_key, message_callback) {
                Ok(_) => {}
                Err(e) => {
                    message!(message_callback, "Failed to set '{}': {}", set_key, e);
                    failures += 1;
                }
            }
        }

        if failures > 0 {
            message!(
                message_callback,
                "{} {}",
                failures,
                style("key(s) failed to be set").red()
            );
        }

        Ok(())
    }

    /// Gets multiple configuration values in bulk.
    pub fn get_bulk<H>(
        &self,
        get_keys: &[String],
        message_callback: &mut Option<H>,
    ) -> Result<Vec<(String, String)>>
    where
        H: FnMut(&str),
    {
        let mut results = Vec::new();

        for get_key in get_keys {
            match self.get_key(get_key, message_callback) {
                Ok(value) => {
                    results.push((get_key.clone(), value));
                }
                Err(e) => {
                    message!(
                        message_callback,
                        "{} '{}': {}",
                        style("Failed to get").red(),
                        get_key,
                        e
                    );
                }
            }
        }

        Ok(results)
    }

    /// Parses a set_key string in the format "key.path=value" into (key_path, value).
    fn parse_set_key(set_key: &str) -> Result<(String, String)> {
        let parts: Vec<&str> = set_key.splitn(2, '=').collect();

        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid set_key format. Expected 'key.path=value', got '{}'",
                set_key
            ));
        }

        let key_path = parts[0].trim();
        let value = parts[1].trim();

        if key_path.is_empty() {
            return Err(anyhow::anyhow!("Key path cannot be empty"));
        }

        Ok((key_path.to_string(), value.to_string()))
    }

    /// Formats a JSON value as a string for display.
    fn format_value(value: &toml::Value) -> String {
        match value {
            toml::Value::String(s) => s.clone(),
            toml::Value::Integer(i) => i.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::Boolean(b) => b.to_string(),
            toml::Value::Table(_) | toml::Value::Array(_) => {
                toml::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
            }
            toml::Value::Datetime(dt) => dt.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigUpdater;
    use crate::services::storage::config_storage::ConfigStorage;
    use std::path::{Path, PathBuf};
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

    fn cleanup(path: &Path) -> io::Result<()> {
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
}
