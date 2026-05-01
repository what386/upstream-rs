use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageMetadata {
    #[serde(default)]
    pub pin_reason: Option<String>,
}
