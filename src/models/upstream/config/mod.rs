pub mod download;
pub mod logging;
pub mod rollback;
pub mod upgrade;

use serde::{Deserialize, Serialize};

pub use {
    download::DownloadConfig,
    logging::LoggingConfig,
    logging::LoggingLevel,
    rollback::RollbackConfig,
    upgrade::UpgradeConfig
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub download: DownloadConfig,
    pub upgrade: UpgradeConfig,
    pub rollback: RollbackConfig,
    pub logging: LoggingConfig,
}
