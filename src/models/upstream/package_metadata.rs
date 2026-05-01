use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageSidecarMetadata {
    #[serde(default)]
    pub pin_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadataFile {
    pub version: u32,
    pub packages: HashMap<String, PackageSidecarMetadata>,
}

impl Default for PackageMetadataFile {
    fn default() -> Self {
        Self {
            version: 1,
            packages: HashMap::new(),
        }
    }
}
