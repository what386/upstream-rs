use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::common::{
    enums::{Channel, Filetype, Provider},
    version::Version,
};
use crate::models::provider::Release;
use crate::providers::pattern_matcher::PatternTable;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstallType {
    Release,
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
    pub install_type: InstallType,
    pub build_branch: Option<String>,
    pub build_commit: Option<String>,

    pub is_pinned: bool,
    #[serde(default)]
    pub match_pattern: PatternTable,
    #[serde(default)]
    pub exclude_pattern: PatternTable,
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
            match_pattern: PatternTable::from_cli_arg(match_pattern),
            exclude_pattern: PatternTable::from_cli_arg(exclude_pattern),
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

    pub fn is_update_available(&self, release: &Release) -> bool {
        if self.channel == Channel::Nightly {
            return release.published_at > self.last_upgraded;
        }

        if release.version.is_unknown() {
            return release.published_at > self.last_upgraded;
        }

        release.version.is_newer_than(&self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::{InstallType, Package};
    use crate::models::{
        common::{
            Version,
            enums::{Channel, Filetype, Provider},
        },
        provider::Release,
    };
    use crate::providers::pattern_matcher::PatternTable;
    use chrono::{Duration, TimeZone, Utc};

    fn update_test_package(version: Version, channel: Channel) -> Package {
        let mut package = Package::with_defaults(
            "tool".to_string(),
            "owner/tool".to_string(),
            Filetype::Archive,
            None,
            None,
            channel,
            Provider::Github,
            None,
        );
        package.version = version;
        package.last_upgraded = Utc
            .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
            .single()
            .expect("valid timestamp");
        package
    }

    fn update_test_release(version: Version, published_offset: Duration) -> Release {
        let base = Utc
            .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
            .single()
            .expect("valid timestamp");
        Release {
            id: 1,
            tag: version.to_string(),
            name: version.to_string(),
            body: String::new(),
            is_draft: false,
            is_prerelease: false,
            assets: Vec::new(),
            version,
            published_at: base + published_offset,
        }
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
        b.match_pattern = PatternTable::from_patterns(["x86_64"]);
        assert!(a.is_same_as(&b));

        a.name = "rg".to_string();
        assert!(!a.is_same_as(&b));
    }

    #[test]
    fn stable_release_uses_semver_when_version_is_known() {
        let package = update_test_package(Version::new(1, 0, 0, false), Channel::Stable);

        assert!(package.is_update_available(&update_test_release(
            Version::new(1, 0, 1, false),
            Duration::seconds(-1)
        )));
        assert!(!package.is_update_available(&update_test_release(
            Version::new(1, 0, 0, false),
            Duration::days(1)
        )));
    }

    #[test]
    fn stable_unknown_release_uses_published_timestamp() {
        let package = update_test_package(Version::new(0, 0, 0, false), Channel::Stable);

        assert!(package.is_update_available(&update_test_release(
            Version::new(0, 0, 0, false),
            Duration::seconds(1)
        )));
        assert!(!package.is_update_available(&update_test_release(
            Version::new(0, 0, 0, false),
            Duration::seconds(0)
        )));
    }

    #[test]
    fn nightly_release_uses_published_timestamp() {
        let package = update_test_package(Version::new(9, 9, 9, false), Channel::Nightly);

        assert!(package.is_update_available(&update_test_release(
            Version::new(1, 0, 0, false),
            Duration::seconds(1)
        )));
        assert!(!package.is_update_available(&update_test_release(
            Version::new(99, 0, 0, false),
            Duration::seconds(0)
        )));
    }
}
