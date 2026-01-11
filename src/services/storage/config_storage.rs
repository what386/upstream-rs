use anyhow::Result;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fs, io};
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

        let toml_str = fs::read_to_string(&self.config_file)
            .map_err(|e| io::Error::other(format!("Failed to load config: {}", e)))?;

        self.config = toml::from_str(&toml_str).unwrap_or_default();
        Ok(())
    }

    /// Saves the current configuration to config.toml.
    pub fn save_config(&self) -> Result<()> {
        let toml = toml::to_string_pretty(&self.config)
            .map_err(|e| io::Error::other(format!("Failed to serialize config: {}", e)))?;

        fs::write(&self.config_file, toml)
            .map_err(|e| io::Error::other(format!("Failed to save config: {}", e)))?;

        Ok(())
    }

    pub fn get_config(&self) -> &AppConfig {
        &self.config
    }

    pub fn get_mut_config(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    /// Sets a configuration value at the given key path (e.g., "github.api_token").
    pub fn try_set_value(&mut self, key_path: &str, value: &str) -> Result<(), String> {
        if key_path.trim().is_empty() {
            return Err("Key path cannot be empty".into());
        }

        let mut root = toml::Value::try_from(&self.config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        let keys: Vec<&str> = key_path.split('.').collect();
        let (path, final_key) = keys.split_at(keys.len() - 1);

        let mut current = root.as_table_mut().ok_or("Config root is not a table")?;

        for key in path {
            current = current
                .get_mut(*key)
                .and_then(toml::Value::as_table_mut)
                .ok_or_else(|| format!("Key path not found: {}", key_path))?;
        }

        let parsed_value = self.convert_value(value)?;
        current.insert(final_key[0].to_string(), parsed_value);

        self.config = root
            .try_into()
            .map_err(|e| format!("Failed to update config: {}", e))?;

        self.save_config()
            .map_err(|e| format!("Failed to save config: {}", e))
    }

    /// Gets a configuration value at the given key path.
    pub fn try_get_value<T>(&self, key_path: &str) -> Result<T, String>
    where
        T: DeserializeOwned,
    {
        let value = self.get_value(key_path)?;
        value
            .clone()
            .try_into()
            .map_err(|e| format!("Failed to deserialize '{}': {}", key_path, e))
    }

    fn get_value(&self, key_path: &str) -> Result<toml::Value, String> {
        let root = toml::Value::try_from(&self.config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        let mut current = &root;
        for key in key_path.split('.') {
            current = current
                .get(key)
                .ok_or_else(|| format!("Key path not found: {}", key_path))?;
        }

        Ok(current.clone())
    }

    /// Gets all configuration keys and values as flattened dot-notation paths.
    pub fn get_flattened_config(&self) -> HashMap<String, String> {
        let root =
            toml::Value::try_from(&self.config).unwrap_or(toml::Value::Table(Default::default()));
        self.flatten_value(&root, "", 10, 0)
    }

    /// Resets all configuration to defaults.
    pub fn reset_to_defaults(&mut self) -> Result<()> {
        self.config = AppConfig::default();
        self.save_config()
    }

    fn flatten_value(
        &self,
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
                    result.extend(self.flatten_value(
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

    fn convert_value(&self, value: &str) -> Result<toml::Value, String> {
        // Try TOML literal first
        if let Ok(parsed) = value.parse::<toml::Value>() {
            return Ok(parsed);
        }

        // Fallback to string
        Ok(toml::Value::String(value.to_string()))
    }
}
