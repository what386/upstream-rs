use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IntegrationConfig {
    /// Discover and install bundled systemd user units.
    pub systemd_units: bool,
    /// Refresh systemd's user-unit definitions after managed unit files change.
    pub systemd_daemon_reload: bool,
    /// Discover and install bundled shell completions.
    pub shell_completions: bool,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            systemd_units: true,
            systemd_daemon_reload: true,
            shell_completions: true,
        }
    }
}
