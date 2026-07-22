pub mod concurrency;
pub mod download;
pub mod logging;
pub mod registry;
pub mod rollback;

use serde::{Deserialize, Serialize};

pub use {
    concurrency::ConcurrencyConfig, download::DownloadConfig, logging::LoggingConfig,
    logging::LoggingLevel, registry::RegistryConfig, rollback::RollbackConfig,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub download: DownloadConfig,
    pub concurrency: ConcurrencyConfig,
    pub rollback: RollbackConfig,
    pub logging: LoggingConfig,
    pub registry: RegistryConfig,
}
