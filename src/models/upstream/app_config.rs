use crate::models::common::enums::CompressionLevel;
use crate::services::trust::{CosignPublicKey, MinisignPublicKey, TrustedSignatureKeys};
use serde::{Deserialize, Serialize};

const MB: u64 = 1024 * 1024;

const LOW_PARALLEL_DOWNLOAD_SIZE_MB: u64 = 16;
const HIGH_PARALLEL_DOWNLOAD_SIZE_MB: u64 = 64;
const LOW_PARALLEL_DOWNLOADS: usize = 2;
const HIGH_PARALLEL_DOWNLOADS: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderConfig {
    pub api_token: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RollbackConfig {
    pub compression_level: CompressionLevel,
    pub stored_artifacts: u32,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            compression_level: CompressionLevel::None,
            stored_artifacts: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadConfig {
    pub low_threshold_mb: u64,
    pub high_threshold_mb: u64,
    pub low_threads: usize,
    pub high_threads: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            low_threshold_mb: LOW_PARALLEL_DOWNLOAD_SIZE_MB,
            high_threshold_mb: HIGH_PARALLEL_DOWNLOAD_SIZE_MB,
            low_threads: LOW_PARALLEL_DOWNLOADS,
            high_threads: HIGH_PARALLEL_DOWNLOADS,
        }
    }
}

impl DownloadConfig {
    pub fn low_threshold_bytes(self) -> u64 {
        self.low_threshold_mb.saturating_mul(MB)
    }

    pub fn high_threshold_bytes(self) -> u64 {
        self.high_threshold_mb.saturating_mul(MB)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub github: ProviderConfig,
    pub gitlab: ProviderConfig,
    pub gitea: ProviderConfig,
    pub download: DownloadConfig,
    pub trust: TrustConfig,
    pub rollback: RollbackConfig,
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
