use serde::{Deserialize, Serialize};
use crate::services::trust::MinisignPublicKey;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[serde(default)]
pub struct MinisignKeyConfig {
    pub id: Option<String>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TrustConfig {
    pub minisign_public_keys: Vec<MinisignKeyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub github: ProviderConfig,
    pub gitlab: ProviderConfig,
    pub gitea: ProviderConfig,
    pub trust: TrustConfig,
}

impl AppConfig {
    pub fn trusted_minisign_keys(&self) -> Vec<MinisignPublicKey> {
        self.trust
            .minisign_public_keys
            .iter()
            .map(|k| MinisignPublicKey {
                id: k.id.clone(),
                key: k.key.clone(),
            })
            .collect()
    }
}
