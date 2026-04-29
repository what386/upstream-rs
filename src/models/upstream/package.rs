use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::common::{
    enums::{Channel, Filetype, Provider},
    version::Version,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum InstallType {
    #[default]
    #[serde(alias = "release")]
    Release,
    #[serde(alias = "build")]
    Build,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub repo_slug: String,

    pub filetype: Filetype,
    pub version: Version,
    pub channel: Channel,
    pub provider: Provider,
    pub base_url: Option<String>,
    #[serde(default)]
    pub install_type: InstallType,
    #[serde(default)]
    pub build_branch: Option<String>,
    #[serde(default)]
    pub build_commit: Option<String>,

    pub is_pinned: bool,
    pub match_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
    pub icon_path: Option<PathBuf>,
    pub install_path: Option<PathBuf>,
    pub exec_path: Option<PathBuf>,

    pub last_upgraded: DateTime<Utc>,
}

impl Package {
    #[allow(clippy::too_many_arguments)]
    pub fn with_defaults(
        name: String,
        repo_slug: String,
        filetype: Filetype,
        match_pattern: Option<String>,
        exclude_pattern: Option<String>,
        channel: Channel,
        provider: Provider,
        base_url: Option<String>,
    ) -> Self {
        Self {
            name,
            repo_slug,

            filetype,
            version: Version::new(0, 0, 0, false),
            channel,
            provider,
            base_url,
            install_type: InstallType::Release,
            build_branch: None,
            build_commit: None,

            is_pinned: false,
            match_pattern,
            exclude_pattern,
            icon_path: None,
            install_path: None,
            exec_path: None,

            last_upgraded: Utc::now(),
        }
    }

    pub fn is_same_as(&self, other: &Package) -> bool {
        self.provider == other.provider
            && self.repo_slug == other.repo_slug
            && self.channel == other.channel
            && self.name == other.name
            && self.base_url == other.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::{InstallType, Package};
    use crate::models::common::enums::{Channel, Filetype, Provider};

    #[test]
    fn with_defaults_sets_expected_base_state() {
        let pkg = Package::with_defaults(
            "bat".to_string(),
            "sharkdp/bat".to_string(),
            Filetype::Auto,
            Some("linux".to_string()),
            Some("debug".to_string()),
            Channel::Stable,
            Provider::Github,
            None,
        );

        assert_eq!(pkg.version.major, 0);
        assert!(!pkg.is_pinned);
        assert_eq!(pkg.install_type, InstallType::Release);
        assert!(pkg.install_path.is_none());
        assert!(pkg.exec_path.is_none());
        assert_eq!(pkg.match_pattern.as_deref(), Some("linux"));
        assert_eq!(pkg.exclude_pattern.as_deref(), Some("debug"));
    }

    #[test]
    fn is_same_as_uses_identity_fields_only() {
        let mut a = Package::with_defaults(
            "ripgrep".to_string(),
            "BurntSushi/ripgrep".to_string(),
            Filetype::Archive,
            None,
            None,
            Channel::Stable,
            Provider::Github,
            Some("https://api.github.com".to_string()),
        );
        let mut b = a.clone();
        b.version.major = 99;
        b.is_pinned = true;
        b.install_type = InstallType::Build;
        b.match_pattern = Some("x86_64".to_string());
        assert!(a.is_same_as(&b));

        a.name = "rg".to_string();
        assert!(!a.is_same_as(&b));
    }

    #[test]
    fn deserialize_defaults_install_type_to_release_for_legacy_records() {
        let legacy = r#"{
            "name":"tool",
            "repo_slug":"owner/tool",
            "filetype":"Binary",
            "version":{"major":1,"minor":2,"patch":3,"is_prerelease":false},
            "channel":"Stable",
            "provider":"Github",
            "base_url":null,
            "is_pinned":false,
            "match_pattern":null,
            "exclude_pattern":null,
            "icon_path":null,
            "install_path":null,
            "exec_path":null,
            "last_upgraded":"2026-01-01T00:00:00Z"
        }"#;
        let pkg: Package = serde_json::from_str(legacy).expect("deserialize legacy package");
        assert_eq!(pkg.install_type, InstallType::Release);
    }

    #[test]
    fn deserialize_accepts_legacy_lowercase_install_type_values() {
        let legacy_lowercase = r#"{
            "name":"tool",
            "repo_slug":"owner/tool",
            "filetype":"Binary",
            "version":{"major":1,"minor":2,"patch":3,"is_prerelease":false},
            "channel":"Stable",
            "provider":"Github",
            "base_url":null,
            "install_type":"release",
            "is_pinned":false,
            "match_pattern":null,
            "exclude_pattern":null,
            "icon_path":null,
            "install_path":null,
            "exec_path":null,
            "last_upgraded":"2026-01-01T00:00:00Z"
        }"#;
        let pkg: Package =
            serde_json::from_str(legacy_lowercase).expect("deserialize lowercase install type");
        assert_eq!(pkg.install_type, InstallType::Release);
    }
}
