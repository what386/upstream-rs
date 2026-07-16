use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml;

use crate::models::upstream::AppConfig;
use crate::utils::filesystem::atomic_ops::write_atomic;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Debug)]
pub struct ConfigStorage {
    config: AppConfig,
    config_file: PathBuf,
}

impl ConfigStorage {
    pub fn new(config_file: &Path) -> Result<Self> {
        let mut storage = Self {
            config: AppConfig::default(),
            config_file: config_file.to_path_buf(),
        };

        storage.load_config()?;
        Ok(storage)
    }

    /// Loads configuration from config.toml if it exists.
    /// If missing, keep in-memory defaults without creating a file.
    pub fn load_config(&mut self) -> Result<()> {
        if !self.config_file.exists() {
            return Ok(());
        }

        let toml_str =
            fs::read_to_string(&self.config_file).context("Failed to load config file")?;

        self.config = toml::from_str(&toml_str).context("Tried to parse an invalid config")?;
        Ok(())
    }

    /// Saves the current configuration to config.toml.
    pub fn save_config(&self) -> Result<()> {
        let toml = toml::to_string_pretty(&self.config).context("Failed to serialize config")?;

        write_atomic(&self.config_file, toml.as_bytes()).with_context(|| {
            format!("Failed to save config to '{}'", self.config_file.display())
        })?;

        #[cfg(unix)]
        set_config_permissions(&self.config_file)?;

        Ok(())
    }

    pub fn get_config(&self) -> &AppConfig {
        &self.config
    }

    pub fn replace_config(&mut self, config: AppConfig) -> Result<()> {
        self.config = config;
        self.save_config()
    }

    /// Sets a configuration value at the given key path (e.g., "download.low_threads").
    pub fn try_set_value(&mut self, key_path: &str, value: &str) -> Result<()> {
        let key_path = key_path.trim();

        if key_path.is_empty() {
            return Err(anyhow!("Key path cannot be empty"));
        }

        let mut root = public_config_value(&self.config).context("Failed to serialize config")?;

        let keys: Vec<&str> = key_path.split('.').collect();
        let (path, final_key) = keys.split_at(keys.len() - 1);

        let mut current = root
            .as_table_mut()
            .ok_or_else(|| anyhow!("Config root is not a table"))?;

        for key in path {
            current = current
                .get_mut(*key)
                .and_then(toml::Value::as_table_mut)
                .ok_or_else(|| anyhow!("Key path not found: {}", key_path))?;
        }

        let parsed_value = self.convert_value(value)?;
        current.insert(final_key[0].to_string(), parsed_value);

        self.config = root.try_into().context("Failed to update config")?;

        self.save_config().context("Failed to save config")
    }

    /// Gets a configuration value at the given key path.
    pub fn try_get_value<T>(&self, key_path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let value = self.get_value(key_path)?;
        value
            .clone()
            .try_into()
            .with_context(|| format!("Failed to deserialize '{}'", key_path))
    }

    fn get_value(&self, key_path: &str) -> Result<toml::Value> {
        let root = public_config_value(&self.config).context("Failed to serialize config")?;

        let mut current = &root;
        for key in key_path.split('.') {
            current = current
                .get(key)
                .ok_or_else(|| anyhow!("Key path not found: {}", key_path))?;
        }

        Ok(current.clone())
    }

    /// Gets all configuration keys and values as flattened dot-notation paths.
    pub fn get_flattened_config(&self) -> HashMap<String, String> {
        let root =
            public_config_value(&self.config).unwrap_or(toml::Value::Table(Default::default()));
        Self::flatten_value(&root, "", 10, 0)
    }

    /// Resets all configuration to defaults.
    pub fn reset_to_defaults(&mut self) -> Result<()> {
        self.config = AppConfig::default();
        self.save_config()
    }

    fn flatten_value(
        value: &toml::Value,
        prefix: &str,
        max_depth: usize,
        current_depth: usize,
    ) -> HashMap<String, String> {
        let mut result = HashMap::new();

        if current_depth >= max_depth {
            return result;
        }

        match value {
            toml::Value::String(s) => {
                result.insert(prefix.to_string(), s.clone());
            }
            toml::Value::Integer(i) => {
                result.insert(prefix.to_string(), i.to_string());
            }
            toml::Value::Float(f) => {
                result.insert(prefix.to_string(), f.to_string());
            }
            toml::Value::Boolean(b) => {
                result.insert(prefix.to_string(), b.to_string());
            }
            toml::Value::Table(table) => {
                for (key, val) in table {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    result.extend(Self::flatten_value(
                        val,
                        &new_prefix,
                        max_depth,
                        current_depth + 1,
                    ));
                }
            }
            _ => {}
        }

        result
    }

    fn convert_value(&self, value: &str) -> Result<toml::Value> {
        // Try TOML literal first
        if let Ok(parsed) = value.parse::<toml::Value>() {
            return Ok(parsed);
        }

        // Fallback to string
        Ok(toml::Value::String(value.to_string()))
    }
}

fn public_config_value(config: &AppConfig) -> Result<toml::Value> {
    toml::Value::try_from(config).context("Failed to serialize config")
}

#[cfg(unix)]
fn set_config_permissions(config_file: &Path) -> Result<()> {
    fs::set_permissions(config_file, fs::Permissions::from_mode(0o600))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ConfigStorage;
    use std::path::{Path, PathBuf};
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

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn new_keeps_defaults_in_memory_when_file_missing() {
        let path = temp_config_file("new-default-in-memory");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }

        let storage = ConfigStorage::new(&path).expect("create storage");
        assert!(!path.exists());
        assert_eq!(storage.get_config().download.low_threshold_mb, 16);
        assert_eq!(storage.get_config().download.high_threshold_mb, 64);
        assert_eq!(storage.get_config().download.low_threads, 2);
        assert_eq!(storage.get_config().download.high_threads, 4);
        assert_eq!(storage.get_config().upgrade.check_concurrency, 8);
        assert_eq!(storage.get_config().upgrade.install_concurrency, 4);
        assert!(storage.get_config().logging.enabled);
        assert_eq!(
            storage.get_config().logging.level,
            crate::models::upstream::LoggingLevel::Info
        );
        assert_eq!(storage.get_config().logging.vacuum, 10_000);
        assert_eq!(storage.get_config().logging.max_size_mb, 10);

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
            .try_set_value("download.low_threshold_mb", "8")
            .expect("set low threshold");
        storage
            .try_set_value("download.high_threads", "6")
            .expect("set high threads");
        storage
            .try_set_value("upgrade.check_concurrency", "3")
            .expect("set check concurrency");

        let low_threshold: u64 = storage
            .try_get_value("download.low_threshold_mb")
            .expect("read low threshold");
        let high_threads: usize = storage
            .try_get_value("download.high_threads")
            .expect("read high threads");
        let check_concurrency: usize = storage
            .try_get_value("upgrade.check_concurrency")
            .expect("read check concurrency");

        assert_eq!(low_threshold, 8);
        assert_eq!(high_threads, 6);
        assert_eq!(check_concurrency, 3);

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
        assert!(err.to_string().contains("Key path not found"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn load_accepts_minimal_config() {
        let path = temp_config_file("unversioned");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, "[download]\nlow_threads = 6\n").expect("write config");

        let storage = ConfigStorage::new(&path).expect("config should load");
        assert_eq!(storage.get_config().download.low_threads, 6);

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn load_accepts_logging_config() {
        let path = temp_config_file("logging");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(
            &path,
            "[logging]\nenabled = false\nlevel = 'error'\nvacuum = 50000\nmax_size_mb = 25\n",
        )
        .expect("write config");

        let storage = ConfigStorage::new(&path).expect("config should load");
        assert!(!storage.get_config().logging.enabled);
        assert_eq!(
            storage.get_config().logging.level,
            crate::models::upstream::LoggingLevel::Error
        );
        assert_eq!(storage.get_config().logging.vacuum, 50000);
        assert_eq!(storage.get_config().logging.max_size_mb, 25);

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn load_rejects_legacy_version_key() {
        let path = temp_config_file("legacy-version");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, "version = 999\n\n[download]\nhigh_threads = 6\n").expect("write config");

        let err = ConfigStorage::new(&path).expect_err("legacy version should be rejected");
        assert!(err.to_string().contains("Tried to parse an invalid config"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn save_config_omits_internal_version_key() {
        let path = temp_config_file("save-without-version");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let storage = ConfigStorage::new(&path).expect("create storage");
        storage.save_config().expect("save config");

        let content = fs::read_to_string(&path).expect("read config");
        assert!(!content.contains("version ="));
        assert!(content.contains("[download]"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn load_rejects_config_with_unsupported_auth_table() {
        let path = temp_config_file("unsupported-trust");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, "[github]\napi_token = \"ghp_abc\"\n").expect("write config");

        let err = ConfigStorage::new(&path).expect_err("auth config should be rejected");
        assert!(err.to_string().contains("Tried to parse an invalid config"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn reset_to_defaults_restores_default_values() {
        let path = temp_config_file("reset");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut storage = ConfigStorage::new(&path).expect("create storage");
        storage.reset_to_defaults().expect("reset defaults");

        assert_eq!(storage.get_config().download.low_threads, 2);

        cleanup(&path).expect("cleanup");
    }
}
