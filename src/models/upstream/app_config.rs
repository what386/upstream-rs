use crate::services::trust::{CosignPublicKey, MinisignPublicKey, TrustedSignatureKeys};
use serde::{Deserialize, Serialize};

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
    pub cosign_public_keys: Vec<CosignKeyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CosignKeyConfig {
    pub id: Option<String>,
    pub key: String,
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
    pub fn trusted_signature_keys(&self) -> TrustedSignatureKeys {
        let minisign_public_keys = self
            .trust
            .minisign_public_keys
            .iter()
            .map(|k| MinisignPublicKey {
                id: k.id.clone(),
                key: k.key.clone(),
            })
            .collect();

        let cosign_public_keys = self
            .trust
            .cosign_public_keys
            .iter()
            .map(|k| CosignPublicKey {
                id: k.id.clone(),
                key: k.key.clone(),
            })
            .collect();

        TrustedSignatureKeys {
            minisign_public_keys,
            cosign_public_keys,
        }
    }
}
