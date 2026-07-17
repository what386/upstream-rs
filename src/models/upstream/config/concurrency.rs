use serde::{Deserialize, Serialize};

const CHECK_CONCURRENCY: usize = 8;
const INSTALL_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConcurrencyConfig {
    pub check_concurrency: usize,
    pub install_concurrency: usize,
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            check_concurrency: CHECK_CONCURRENCY,
            install_concurrency: INSTALL_CONCURRENCY,
        }
    }
}

impl ConcurrencyConfig {
    pub fn check_concurrency(self) -> usize {
        self.check_concurrency.max(1)
    }

    pub fn install_concurrency(self) -> usize {
        self.install_concurrency.max(1)
    }
}
