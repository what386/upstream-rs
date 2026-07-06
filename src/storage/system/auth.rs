use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::upstream::AuthenticationConfig;
use crate::utils::filesystem::atomic_ops::write_atomic;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const AUTH_KEYS: &[&str] = &["github.api_token", "gitlab.api_token", "gitea.api_token"];

#[derive(Debug)]
pub struct AuthStorage {
    auth: AuthenticationConfig,
    auth_file: PathBuf,
}

impl AuthStorage {
    pub fn new(auth_file: &Path) -> Result<Self> {
        let mut storage = Self {
            auth: AuthenticationConfig::default(),
            auth_file: auth_file.to_path_buf(),
        };

        storage.load_auth()?;
        Ok(storage)
    }

    /// Loads sensitive provider credentials from auth.toml if it exists.
    pub fn load_auth(&mut self) -> Result<()> {
        if !self.auth_file.exists() {
            return Ok(());
        }

        let toml_str = fs::read_to_string(&self.auth_file).context("Failed to load auth file")?;
        self.auth = toml::from_str(&toml_str).context("Tried to parse an invalid auth file")?;
        Ok(())
    }

    /// Saves the current authentication state to auth.toml.
    pub fn save_auth(&self) -> Result<()> {
        let toml = toml::to_string_pretty(&self.auth).context("Failed to serialize auth")?;

        write_atomic(&self.auth_file, toml.as_bytes())
            .with_context(|| format!("Failed to save auth to '{}'", self.auth_file.display()))?;

        #[cfg(unix)]
        set_auth_permissions(&self.auth_file)?;

        Ok(())
    }

    pub fn get_auth(&self) -> &AuthenticationConfig {
        &self.auth
    }

    pub fn replace_auth(&mut self, auth: AuthenticationConfig) -> Result<()> {
        self.auth = auth;
        self.save_auth()
    }

    pub fn try_set_value(&mut self, key_path: &str, value: &str) -> Result<()> {
        let key_path = key_path.trim();

        if key_path.is_empty() {
            return Err(anyhow!("Key path cannot be empty"));
        }

        if !Self::is_auth_key(key_path) {
            return Err(anyhow!("Unsupported auth key path: {}", key_path));
        }

        let mut root = public_auth_value(&self.auth).context("Failed to serialize auth")?;
        let keys: Vec<&str> = key_path.split('.').collect();
        let (path, final_key) = keys.split_at(keys.len() - 1);

        let mut current = root
            .as_table_mut()
            .ok_or_else(|| anyhow!("Auth root is not a table"))?;

        for key in path {
            current = current
                .get_mut(*key)
                .and_then(toml::Value::as_table_mut)
                .ok_or_else(|| anyhow!("Key path not found: {}", key_path))?;
        }

        let parsed_value = self.convert_value(value)?;
        current.insert(final_key[0].to_string(), parsed_value);

        self.auth = root.try_into().context("Failed to update auth")?;
        self.save_auth().context("Failed to save auth")
    }

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

    pub fn get_flattened_auth(&self) -> HashMap<String, String> {
        let root = public_auth_value(&self.auth).unwrap_or(toml::Value::Table(Default::default()));
        Self::flatten_value(&root, "", 10, 0)
    }

    pub fn reset_to_defaults(&mut self) -> Result<()> {
        self.auth = AuthenticationConfig::default();
        self.save_auth()
    }

    pub fn is_auth_key(key_path: &str) -> bool {
        AUTH_KEYS.contains(&key_path)
    }

    fn get_value(&self, key_path: &str) -> Result<toml::Value> {
        let root = public_auth_value(&self.auth).context("Failed to serialize auth")?;

        let mut current = &root;
        for key in key_path.split('.') {
            current = current
                .get(key)
                .ok_or_else(|| anyhow!("Key path not found: {}", key_path))?;
        }

        Ok(current.clone())
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
        if let Ok(parsed) = value.parse::<toml::Value>() {
            return Ok(parsed);
        }

        Ok(toml::Value::String(value.to_string()))
    }
}

fn public_auth_value(auth: &AuthenticationConfig) -> Result<toml::Value> {
    toml::Value::try_from(auth).context("Failed to serialize auth")
}

#[cfg(unix)]
fn set_auth_permissions(auth_file: &Path) -> Result<()> {
    fs::set_permissions(auth_file, fs::Permissions::from_mode(0o600))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AuthStorage;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn temp_auth_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-auth-test-{name}-{nanos}"))
            .join("auth.toml")
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn new_keeps_defaults_in_memory_when_file_missing() {
        let path = temp_auth_file("new-default-in-memory");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }

        let storage = AuthStorage::new(&path).expect("create storage");
        assert!(!path.exists());
        assert!(storage.get_auth().github.api_token.is_none());

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn set_and_get_auth_values_updates_storage() {
        let path = temp_auth_file("set-get");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        let mut storage = AuthStorage::new(&path).expect("create storage");

        storage
            .try_set_value("github.api_token", "\"ghp_abc\"")
            .expect("set github token");
        storage
            .try_set_value("gitlab.api_token", "\"glpat_abc\"")
            .expect("set gitlab token");

        let github: Option<String> = storage
            .try_get_value("github.api_token")
            .expect("read github token");
        let gitlab: Option<String> = storage
            .try_get_value("gitlab.api_token")
            .expect("read gitlab token");

        assert_eq!(github.as_deref(), Some("ghp_abc"));
        assert_eq!(gitlab.as_deref(), Some("glpat_abc"));

        cleanup(&path).expect("cleanup");
    }

    #[test]
    fn load_rejects_unknown_auth_keys() {
        let path = temp_auth_file("unknown-key");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, "[github]\napi_token = \"ghp_abc\"\nextra = true\n").expect("write auth");

        let err = AuthStorage::new(&path).expect_err("auth config should be rejected");
        assert!(
            err.to_string()
                .contains("Tried to parse an invalid auth file")
        );

        cleanup(&path).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn save_auth_sets_file_permissions_to_600() {
        let path = temp_auth_file("permissions");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }

        let mut storage = AuthStorage::new(&path).expect("create storage");
        storage
            .try_set_value("gitea.api_token", "\"token\"")
            .expect("set token");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        cleanup(&path).expect("cleanup");
    }
}
