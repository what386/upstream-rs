use std::collections::HashMap;
use std::{fs, io};
use std::path::Path;
use std::path::PathBuf;
use anyhow::Result;

use crate::models::upstream::AppConfig;

pub struct ConfigStorage {
    config: AppConfig,
    config_file: PathBuf
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

    /// Loads configuration from config.json, or creates default if it doesn't exist.
    pub fn load_config(&mut self) -> Result<()> {
        if !Path::new(&self.config_file).exists() {
            return self.save_config();
        }

        let json = fs::read_to_string(&self.config_file)
            .map_err(|e| io::Error::other(format!("Failed to load config: {}", e)))?;

        self.config = serde_json::from_str(&json)
            .unwrap_or_default();

        Ok(())
    }

    /// Saves the current configuration to config.json.
    pub fn save_config(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|e| io::Error::other(format!("Failed to serialize config: {}", e)))?;

        fs::write(&self.config_file, json)
            .map_err(|e| io::Error::other(format!("Failed to save config: {}", e)))?;

        Ok(())
    }

    /// Gets a configuration value at the given key path (e.g., "github.apiToken" or "rateLimit").
    pub fn try_get_value(&self, key_path: &str) -> Result<serde_json::Value, String> {
        if key_path.trim().is_empty() {
            return Err("Key path cannot be empty".to_string());
        }

        let config_json = serde_json::to_value(&self.config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        let keys: Vec<&str> = key_path.split('.').collect();
        let mut current = &config_json;

        for (i, key) in keys.iter().enumerate() {
            current = current.get(key)
                .ok_or_else(|| format!("Key path not found: {}", keys[..=i].join(".")))?;
        }

        Ok(current.clone())
    }

    /// Sets a configuration value at the given key path (e.g., "github.apiToken" or "rateLimit").
    pub fn try_set_value(&mut self, key_path: &str, value: &str) -> Result<(), String> {
        if key_path.trim().is_empty() {
            return Err("Key path cannot be empty".to_string());
        }

        let mut config_json = serde_json::to_value(&self.config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        let keys: Vec<&str> = key_path.split('.').collect();
        let mut current = &mut config_json;

        // Navigate to parent
        for (i, key) in keys[..keys.len() - 1].iter().enumerate() {
            current = current.get_mut(key)
                .ok_or_else(|| format!("Key path not found: {}", keys[..=i].join(".")))?;

            if current.is_null() {
                return Err(format!("Cannot navigate through null value at: {}", key));
            }
        }

        let final_key = keys[keys.len() - 1];
        let target = current.get_mut(final_key)
            .ok_or_else(|| format!("Unknown key: {}", key_path))?;

        // Try to convert the string value to the appropriate JSON type
        *target = self.convert_value(value, target)?;

        // Deserialize back to config
        self.config = serde_json::from_value(config_json)
            .map_err(|e| format!("Failed to update config: {}", e))?;

        self.save_config()
            .map_err(|e| format!("Failed to save config: {}", e))
    }

    /// Gets all configuration keys and values as a flattened dictionary with dot-notation paths.
    pub fn get_flattened_config(&self) -> HashMap<String, String> {
        let config_json = serde_json::to_value(&self.config).unwrap_or(serde_json::Value::Null);
        self.flatten_value(&config_json, "", 10, 0)
    }

    /// Resets all configuration to defaults.
    pub fn reset_to_defaults(&mut self) -> Result<()> {
        self.config = AppConfig::default();
        self.save_config()
    }

    fn flatten_value(
        &self,
        value: &serde_json::Value,
        prefix: &str,
        max_depth: usize,
        current_depth: usize,
    ) -> HashMap<String, String> {
        let mut result = HashMap::new();

        if current_depth >= max_depth {
            return result;
        }

        match value {
            serde_json::Value::Null => {
                result.insert(prefix.to_string(), "null".to_string());
            }
            serde_json::Value::Bool(b) => {
                result.insert(prefix.to_string(), b.to_string());
            }
            serde_json::Value::Number(n) => {
                result.insert(prefix.to_string(), n.to_string());
            }
            serde_json::Value::String(s) => {
                result.insert(prefix.to_string(), s.clone());
            }
            serde_json::Value::Object(obj) => {
                for (key, val) in obj {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    let nested = self.flatten_value(val, &new_prefix, max_depth, current_depth + 1);
                    result.extend(nested);
                }
            }
            serde_json::Value::Array(_) => {
                // Skip arrays for flattening
            }
        }

        result
    }

    fn convert_value(&self, value: &str, target: &serde_json::Value) -> Result<serde_json::Value, String> {
        // Try to parse as JSON first (handles numbers, bools, nulls, strings)
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(value) {
            return Ok(parsed);
        }

        // Fallback to string if it's not valid JSON
        Ok(serde_json::Value::String(value.to_string()))
    }
}
