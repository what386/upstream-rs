use crate::models::common::enums::CompressionLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RollbackConfig {
    pub enabled: bool,
    pub compression_level: CompressionLevel,
    pub stored_artifacts: u32,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            compression_level: CompressionLevel::None,
            stored_artifacts: 1,
        }
    }
}
