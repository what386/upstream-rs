use serde::{Deserialize, Serialize};

pub const DEFAULT_INDEX_URL: &str =
    "https://raw.githubusercontent.com/what386/upstream-rs/main/registry/index.min.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RegistryConfig {
    pub index_url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            index_url: DEFAULT_INDEX_URL.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_INDEX_URL, RegistryConfig};

    #[test]
    fn defaults_to_upstream_minified_index() {
        assert_eq!(RegistryConfig::default().index_url, DEFAULT_INDEX_URL);
        assert!(DEFAULT_INDEX_URL.ends_with("/registry/index.min.json"));
    }
}
