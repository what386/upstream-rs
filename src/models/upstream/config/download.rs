use serde::{Deserialize, Serialize};

const MB: u64 = 1024 * 1024;

const LOW_PARALLEL_DOWNLOAD_SIZE_MB: u64 = 16;
const HIGH_PARALLEL_DOWNLOAD_SIZE_MB: u64 = 64;
const LOW_PARALLEL_DOWNLOADS: usize = 2;
const HIGH_PARALLEL_DOWNLOADS: usize = 4;

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
