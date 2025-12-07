use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_token: Option<String>,
    pub rate_limit: u32,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_token: None,
            rate_limit: 5000,
        }
    }
}

pub struct AppConfig {
    pub github: ProviderConfig,
    pub gitlab: ProviderConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            github: ProviderConfig::default(),
            gitlab: ProviderConfig::default(),
        }
    }
}
