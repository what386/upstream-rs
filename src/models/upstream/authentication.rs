use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderAuthentication {
    pub api_token: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthenticationConfig {
    pub github: ProviderAuthentication,
    pub gitlab: ProviderAuthentication,
    pub gitea: ProviderAuthentication,
}
