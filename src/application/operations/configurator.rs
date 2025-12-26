use crate::services::storage::config_storage::ConfigStorage;
use anyhow::Result;
use console::style;

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
    /// Example: "parent.child=value" or "github.apiToken=abc123"
    pub fn set_key<H>(&mut self, set_key: &str, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        let (key_path, value) = Self::parse_set_key(set_key)?;

        message!(message_callback, "Setting '{}' = '{}'", key_path, value);

        self.config_storage
            .try_set_value(&key_path, &value)
            .map_err(|e| anyhow::anyhow!("Failed to set config value: {}", e))?;

        message!(
            message_callback,
            "{}",
            style("Configuration updated successfully").green()
        );

        Ok(())
    }

    /// Gets a configuration value using dot-notation key path.
    /// Example: "parent.child" or "github.apiToken"
    pub fn get_key<H>(&self, get_key: &str, message_callback: &mut Option<H>) -> Result<String>
    where
        H: FnMut(&str),
    {
        let key_path = get_key.trim();

        if key_path.is_empty() {
            return Err(anyhow::anyhow!("Key path cannot be empty"));
        }

        message!(message_callback, "Getting value for '{}'", key_path);

        let value: serde_json::Value = self
            .config_storage
            .try_get_value(key_path)
            .map_err(|e| anyhow::anyhow!("Failed to get config value: {}", e))?;

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
    fn format_value(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
            }
        }
    }
}
