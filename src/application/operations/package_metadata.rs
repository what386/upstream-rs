use crate::services::storage::package_storage::PackageStorage;
use anyhow::Result;
use console::style;

macro_rules! message {
    ($cb:expr, $($arg:tt)*) => {{
        if let Some(cb) = $cb.as_mut() {
            cb(&format!($($arg)*));
        }
    }};
}

pub struct MetadataManager<'a> {
    package_storage: &'a mut PackageStorage,
}

impl<'a> MetadataManager<'a> {
    pub fn new(package_storage: &'a mut PackageStorage) -> Self {
        Self { package_storage }
    }

    /// Pins a package to its current version, preventing automatic updates.
    pub fn pin_package<H>(&mut self, name: &str, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Pinning package '{}'...", name);

        let package = self
            .package_storage
            .get_mut_package_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        if package.is_pinned {
            message!(
                message_callback,
                "{}",
                style(format!("Package '{}' is already pinned", name)).yellow()
            );
            return Ok(());
        }

        let version = package.version.clone();
        package.is_pinned = true;
        self.package_storage.save_packages()?;

        message!(
            message_callback,
            "{}",
            style(format!(
                "Package '{}' pinned at version {}",
                name, version
            ))
            .green()
        );

        Ok(())
    }

    /// Unpins a package, allowing it to receive automatic updates.
    pub fn unpin_package<H>(&mut self, name: &str, message_callback: &mut Option<H>) -> Result<()>
    where
        H: FnMut(&str),
    {
        message!(message_callback, "Unpinning package '{}'...", name);

        let package = self
            .package_storage
            .get_mut_package_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        if !package.is_pinned {
            message!(
                message_callback,
                "{}",
                style(format!("Package '{}' is not pinned", name)).yellow()
            );
            return Ok(());
        }

        package.is_pinned = false;
        self.package_storage.save_packages()?;

        message!(
            message_callback,
            "{}",
            style(format!("Package '{}' unpinned", name)).green()
        );

        Ok(())
    }

    /// Sets a package metadata field using dot-notation key path.
    /// Example: "is_pinned=true" or "pattern=.*x86_64.*"
    pub fn set_key<H>(
        &mut self,
        name: &str,
        set_key: &str,
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let (key_path, value) = Self::parse_set_key(set_key)?;

        message!(
            message_callback,
            "Setting '{}' for package '{}' = '{}'",
            key_path,
            name,
            value
        );

        // Get the package
        let package = self
            .package_storage
            .get_package_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        // Serialize to JSON for manipulation
        let mut json_value = serde_json::to_value(package)?;

        // Navigate to the field and set it
        Self::set_nested_value(&mut json_value, &key_path, &value)?;

        // Deserialize back to Package
        let updated_package: crate::models::upstream::Package = serde_json::from_value(json_value)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize updated package: {}", e))?;

        // Update in storage
        self.package_storage.add_or_update_package(updated_package)?;

        message!(
            message_callback,
            "{}",
            style("Package metadata updated successfully").green()
        );

        Ok(())
    }

    /// Gets a package metadata field using dot-notation key path.
    /// Example: "is_pinned" or "version"
    pub fn get_key<H>(
        &self,
        name: &str,
        get_key: &str,
        message_callback: &mut Option<H>,
    ) -> Result<String>
    where
        H: FnMut(&str),
    {
        let key_path = get_key.trim();
        if key_path.is_empty() {
            return Err(anyhow::anyhow!("Key path cannot be empty"));
        }

        message!(
            message_callback,
            "Getting value for '{}' from package '{}'",
            key_path,
            name
        );

        // Get the package
        let package = self
            .package_storage
            .get_package_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        // Serialize to JSON for field access
        let json_value = serde_json::to_value(package)?;

        // Navigate to the field
        let value = Self::get_nested_value(&json_value, key_path)?;

        let value_str = Self::format_value(&value);
        message!(
            message_callback,
            "{}.{} = {}",
            name,
            key_path,
            style(&value_str).cyan()
        );

        Ok(value_str)
    }

    /// Sets multiple package metadata fields in bulk.
    pub fn set_bulk<H>(
        &mut self,
        name: &str,
        set_keys: &[String],
        message_callback: &mut Option<H>,
    ) -> Result<()>
    where
        H: FnMut(&str),
    {
        let mut failures = 0;

        for set_key in set_keys {
            match self.set_key(name, set_key, message_callback) {
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

    /// Gets multiple package metadata fields in bulk.
    pub fn get_bulk<H>(
        &self,
        name: &str,
        get_keys: &[String],
        message_callback: &mut Option<H>,
    ) -> Result<Vec<(String, String)>>
    where
        H: FnMut(&str),
    {
        let mut results = Vec::new();

        for get_key in get_keys {
            match self.get_key(name, get_key, message_callback) {
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

    /// Parses a set_key string in the format "key=value" into (key_path, value).
    fn parse_set_key(set_key: &str) -> Result<(String, String)> {
        let parts: Vec<&str> = set_key.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid set_key format. Expected 'key=value', got '{}'",
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

    /// Gets a nested value from JSON using dot notation.
    fn get_nested_value(json: &serde_json::Value, path: &str) -> Result<serde_json::Value> {
        let keys: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for key in keys {
            current = current
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", key))?;
        }

        Ok(current.clone())
    }

    /// Sets a nested value in JSON using dot notation.
    fn set_nested_value(
        json: &mut serde_json::Value,
        path: &str,
        value: &str,
    ) -> Result<()> {
        let keys: Vec<&str> = path.split('.').collect();

        if keys.is_empty() {
            return Err(anyhow::anyhow!("Empty path"));
        }

        let mut current = json;

        // Navigate to the parent of the target field
        for key in &keys[..keys.len() - 1] {
            current = current
                .get_mut(key)
                .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", key))?;
        }

        // Set the final field
        let final_key = keys[keys.len() - 1];
        let target = current
            .get_mut(final_key)
            .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", final_key))?;

        // Parse the value based on the target type
        *target = Self::parse_value_for_type(target, value)?;

        Ok(())
    }

    /// Parses a string value into the appropriate JSON type based on the existing field type.
    fn parse_value_for_type(
        existing: &serde_json::Value,
        value_str: &str,
    ) -> Result<serde_json::Value> {
        match existing {
            serde_json::Value::Bool(_) => {
                let bool_val = value_str.parse::<bool>()
                    .map_err(|_| anyhow::anyhow!("Expected boolean value, got '{}'", value_str))?;
                Ok(serde_json::Value::Bool(bool_val))
            }
            serde_json::Value::Number(_) => {
                if let Ok(int_val) = value_str.parse::<i64>() {
                    Ok(serde_json::json!(int_val))
                } else if let Ok(float_val) = value_str.parse::<f64>() {
                    Ok(serde_json::json!(float_val))
                } else {
                    Err(anyhow::anyhow!("Expected numeric value, got '{}'", value_str))
                }
            }
            serde_json::Value::String(_) => {
                Ok(serde_json::Value::String(value_str.to_string()))
            }
            serde_json::Value::Null => {
                if value_str == "null" {
                    Ok(serde_json::Value::Null)
                } else {
                    // Try to infer type
                    if let Ok(bool_val) = value_str.parse::<bool>() {
                        Ok(serde_json::Value::Bool(bool_val))
                    } else if let Ok(int_val) = value_str.parse::<i64>() {
                        Ok(serde_json::json!(int_val))
                    } else {
                        Ok(serde_json::Value::String(value_str.to_string()))
                    }
                }
            }
            _ => {
                // For objects/arrays, try to parse as JSON
                serde_json::from_str(value_str)
                    .map_err(|_| anyhow::anyhow!("Cannot set complex type from string. Expected JSON, got '{}'", value_str))
            }
        }
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
