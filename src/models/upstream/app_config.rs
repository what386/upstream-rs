use crate::models::common::enums::CompressionLevel;
use serde::{Deserialize, Serialize};

const MB: u64 = 1024 * 1024;

const LOW_PARALLEL_DOWNLOAD_SIZE_MB: u64 = 16;
const HIGH_PARALLEL_DOWNLOAD_SIZE_MB: u64 = 64;
const LOW_PARALLEL_DOWNLOADS: usize = 2;
const HIGH_PARALLEL_DOWNLOADS: usize = 4;
const UPGRADE_CHECK_CONCURRENCY: usize = 8;
const UPGRADE_INSTALL_CONCURRENCY: usize = 4;
const LOGGING_VACUUM: usize = 10_000;
const LOGGING_MAX_SIZE_MB: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl Default for LoggingLevel {
    fn default() -> Self {
        Self::Info
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub enabled: bool,
    pub level: LoggingLevel,
    pub vacuum: usize,
    pub max_size_mb: u64,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: LoggingLevel::Info,
            vacuum: LOGGING_VACUUM,
            max_size_mb: LOGGING_MAX_SIZE_MB,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RollbackConfig {
    pub compression_level: CompressionLevel,
    pub stored_artifacts: u32,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            compression_level: CompressionLevel::Low,
            stored_artifacts: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpgradeConfig {
    pub check_concurrency: usize,
    pub install_concurrency: usize,
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            check_concurrency: UPGRADE_CHECK_CONCURRENCY,
            install_concurrency: UPGRADE_INSTALL_CONCURRENCY,
        }
    }
}

impl UpgradeConfig {
    pub fn check_concurrency(self) -> usize {
        self.check_concurrency.max(1)
    }

    pub fn install_concurrency(self) -> usize {
        self.install_concurrency.max(1)
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub download: DownloadConfig,
    pub upgrade: UpgradeConfig,
    pub rollback: RollbackConfig,
    pub logging: LoggingConfig,
}
