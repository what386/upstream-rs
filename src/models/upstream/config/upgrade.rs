use serde::{Deserialize, Serialize};

const UPGRADE_CHECK_CONCURRENCY: usize = 8;
const UPGRADE_INSTALL_CONCURRENCY: usize = 4;

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
