use serde::{Deserialize, Serialize};

const MB: u64 = 1024 * 1024;

const LOGGING_VACUUM: usize = 10_000;
const LOGGING_MAX_SIZE_MB: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LoggingLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
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

impl LoggingConfig {
    pub fn max_size_bytes(self) -> u64 {
        self.max_size_mb.saturating_mul(MB)
    }
}
