use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub github: ProviderConfig,
    pub gitlab: ProviderConfig,
}

impl AppConfig {
    pub fn github_api_token(&self) -> Option<&str> {
        self.github.api_token.as_deref()
    }

    pub fn github_rate_limit(&self) -> u32 {
        self.github.rate_limit
    }

    pub fn gitlab_api_token(&self) -> Option<&str> {
        self.gitlab.api_token.as_deref()
    }

    pub fn gitlab_rate_limit(&self) -> u32 {
        self.gitlab.rate_limit
    }
}
