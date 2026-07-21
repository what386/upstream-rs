use crate::storage::system::auth::AuthStorage;
use anyhow::Result;
use toml;

pub struct AuthUpdater<'a> {
    auth_storage: &'a mut AuthStorage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSetResult {
    pub key: String,
    pub display_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthBulkSetResult {
    pub applied: Vec<AuthSetResult>,
    pub failures: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthBulkGetResult {
    pub values: Vec<(String, String)>,
    pub failures: Vec<(String, String)>,
}

impl<'a> AuthUpdater<'a> {
    pub fn new(auth_storage: &'a mut AuthStorage) -> Self {
        Self { auth_storage }
    }

    pub fn set_key(&mut self, set_key: &str) -> Result<AuthSetResult> {
        let (key_path, value) = Self::parse_set_key(set_key)?;

        self.auth_storage.try_set_value(&key_path, &value)?;

        Ok(AuthSetResult {
            key: key_path,
            display_value: value,
        })
    }

    pub fn get_key(&self, get_key: &str) -> Result<String> {
        let key_path = get_key.trim();

        if key_path.is_empty() {
            return Err(anyhow::anyhow!("Key path cannot be empty"));
        }

        let value: toml::Value = self.auth_storage.try_get_value(key_path)?;

        Ok(Self::format_value(&value))
    }

    pub fn set_bulk(&mut self, set_keys: &[String]) -> AuthBulkSetResult {
        let mut applied = Vec::new();
        let mut failures = Vec::new();

        for set_key in set_keys {
            match self.set_key(set_key) {
                Ok(result) => applied.push(result),
                Err(err) => failures.push((set_key.clone(), err.to_string())),
            }
        }

        AuthBulkSetResult { applied, failures }
    }

    pub fn get_bulk(&self, get_keys: &[String]) -> AuthBulkGetResult {
        let mut values = Vec::new();
        let mut failures = Vec::new();

        for get_key in get_keys {
            match self.get_key(get_key) {
                Ok(value) => values.push((get_key.clone(), value)),
                Err(err) => failures.push((get_key.clone(), err.to_string())),
            }
        }

        AuthBulkGetResult { values, failures }
    }

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
    use super::AuthUpdater;
    use crate::storage::system::auth::AuthStorage;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, io};

    fn temp_auth_file(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-auth-updater-test-{name}-{nanos}"))
            .join("auth.toml")
    }

    fn cleanup(path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            match fs::remove_dir_all(parent) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    #[test]
    fn parse_set_key_requires_key_value_format() {
        assert!(AuthUpdater::parse_set_key("github.api_token=ghp_abc").is_ok());
        assert!(AuthUpdater::parse_set_key("missing-separator").is_err());
        assert!(AuthUpdater::parse_set_key("   =x").is_err());
    }

    #[test]
    fn set_key_and_get_key_round_trip_value() {
        let auth_file = temp_auth_file("roundtrip");
        fs::create_dir_all(auth_file.parent().expect("auth parent")).expect("create parent");
        let mut storage = AuthStorage::new(&auth_file).expect("create storage");
        let mut updater = AuthUpdater::new(&mut storage);

        updater
            .set_key("github.api_token=ghp_abc")
            .expect("set key");
        let value = updater.get_key("github.api_token").expect("get key");
        assert_eq!(value, "ghp_abc");

        cleanup(&auth_file).expect("cleanup");
    }

    #[test]
    fn set_bulk_continues_after_failures_and_applies_valid_keys() {
        let auth_file = temp_auth_file("bulk");
        fs::create_dir_all(auth_file.parent().expect("auth parent")).expect("create parent");
        let mut storage = AuthStorage::new(&auth_file).expect("create storage");
        let mut updater = AuthUpdater::new(&mut storage);
        let keys = vec![
            "github.api_token=ghp_abc".to_string(),
            "badformat".to_string(),
            "gitlab.api_token=glpat_abc".to_string(),
        ];

        let result = updater.set_bulk(&keys);
        assert_eq!(result.applied.len(), 2);
        assert_eq!(result.failures.len(), 1);
        let github = updater.get_key("github.api_token").expect("github key");
        let gitlab = updater.get_key("gitlab.api_token").expect("gitlab key");
        assert_eq!(github, "ghp_abc");
        assert_eq!(gitlab, "glpat_abc");

        cleanup(&auth_file).expect("cleanup");
    }
}
