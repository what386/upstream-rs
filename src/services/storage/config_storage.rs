use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml;

use crate::models::upstream::AppConfig;

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

    /// Loads configuration from config.toml, or creates default if it doesn't exist.
    pub fn load_config(&mut self) -> Result<()> {
        if !self.config_file.exists() {
            return self.save_config();
        }

        let toml_str =
            fs::read_to_string(&self.config_file).context("Failed to load config file")?;

        self.config = toml::from_str(&toml_str).context("Tried to parse an invalid config")?;
        Ok(())
    }

    /// Saves the current configuration to config.toml.
    pub fn save_config(&self) -> Result<()> {
        let toml = toml::to_string_pretty(&self.config).context("Failed to serialize config")?;

        fs::write(&self.config_file, toml).with_context(|| {
            format!("Failed to save config to '{}'", self.config_file.display())
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.config_file, fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn get_config(&self) -> &AppConfig {
        &self.config
    }

    /// Sets a configuration value at the given key path (e.g., "github.api_token").
    pub fn try_set_value(&mut self, key_path: &str, value: &str) -> Result<()> {
        if key_path.trim().is_empty() {
            return Err(anyhow!("Key path cannot be empty"));
        }

        let mut root = toml::Value::try_from(&self.config).context("Failed to serialize config")?;

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
        let root = toml::Value::try_from(&self.config).context("Failed to serialize config")?;

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
            toml::Value::try_from(&self.config).unwrap_or(toml::Value::Table(Default::default()));
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

#[cfg(test)]
#[path = "../../../tests/services/storage/config_storage.rs"]
mod tests;
