use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::common::{
    enums::{Channel, Filetype, Provider},
    version::{Version, VersionTagTemplate},
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
    #[serde(default)]
    pub release_tag: Option<String>,
    #[serde(default)]
    pub release_published_at: Option<DateTime<Utc>>,
    /// Legacy fallback for metadata written before exact release tags were stored.
    #[serde(default)]
    pub version_tag_template: Option<String>,
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
            release_tag: None,
            release_published_at: None,
            version_tag_template: None,
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
        if self
            .release_tag
            .as_deref()
            .is_some_and(|installed| installed == release.tag)
        {
            return false;
        }

        match release.version.partial_cmp(&self.version) {
            Some(std::cmp::Ordering::Greater) => true,
            Some(_) => false,
            None => match self.release_published_at {
                Some(installed_at) if release.published_at != DateTime::<Utc>::MIN_UTC => {
                    release.published_at > installed_at
                }
                _ if self.release_tag.is_some() => true,
                _ => release.published_at > self.last_upgraded,
            },
        }
    }

    pub fn record_release(&mut self, release: &Release) {
        self.version = release.version.clone();
        if matches!(
            self.provider,
            Provider::Github | Provider::Gitlab | Provider::Gitea
        ) {
            self.release_tag = Some(release.tag.clone());
            self.release_published_at =
                (release.published_at != DateTime::<Utc>::MIN_UTC).then_some(release.published_at);
        } else {
            self.release_tag = None;
            self.release_published_at = None;
        }
        self.version_tag_template = None;
    }

    pub fn installed_release_tag(&self) -> Option<String> {
        self.release_tag
            .clone()
            .or_else(|| self.version_tag_from_template())
    }

    pub fn version_tag_from_template(&self) -> Option<String> {
        let template = self.version_tag_template.as_ref()?;
        VersionTagTemplate::parse(template.clone()).map(|template| template.render(&self.version))
    }

    pub fn version_tag_template_from_tag(tag: &str, version: &Version) -> Option<String> {
        VersionTagTemplate::from_tag(tag, version).map(|template| template.as_str().to_string())
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
        b.version = Version::new(99, 0, 0, false);
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
    fn nightly_release_uses_structural_version_when_known() {
        let package = update_test_package(Version::new(9, 9, 9, false), Channel::Nightly);

        assert!(!package.is_update_available(&update_test_release(
            Version::new(1, 0, 0, false),
            Duration::seconds(1)
        )));
        assert!(package.is_update_available(&update_test_release(
            Version::new(99, 0, 0, false),
            Duration::seconds(0)
        )));
    }

    #[test]
    fn datetime_updates_use_timestamp_and_publication_fallback() {
        let installed = Version::parse("20240203-110809-5046fc22").expect("installed");
        let package = update_test_package(installed, Channel::Stable);

        assert!(package.is_update_available(&update_test_release(
            Version::parse("20240204-000000-aaaaaaaa").expect("later timestamp"),
            Duration::seconds(-1),
        )));
        assert!(!package.is_update_available(&update_test_release(
            Version::parse("20240202-235959-bbbbbbbb").expect("earlier timestamp"),
            Duration::days(1),
        )));
        assert!(package.is_update_available(&update_test_release(
            Version::parse("20240203-110809-cccccccc").expect("revision collision"),
            Duration::seconds(1),
        )));
    }

    #[test]
    fn exact_and_opaque_release_tags_control_update_identity() {
        let mut package = update_test_package(Version::new(0, 0, 0, false), Channel::Stable);
        package.release_tag = Some("snapshot-alpha".to_string());

        let same = update_test_release(Version::new(0, 0, 0, false), Duration::days(1));
        assert!(!package.is_update_available(&Release {
            tag: "snapshot-alpha".to_string(),
            ..same.clone()
        }));

        let changed_without_timestamp = Release {
            tag: "snapshot-beta".to_string(),
            published_at: chrono::DateTime::<Utc>::MIN_UTC,
            ..same
        };
        assert!(package.is_update_available(&changed_without_timestamp));
    }

    #[test]
    fn record_release_keeps_exact_tag_and_publication_time_for_forges() {
        let mut package = update_test_package(Version::new(1, 0, 0, false), Channel::Stable);
        let release = update_test_release(Version::new(1, 2, 3, false), Duration::days(2));

        package.record_release(&Release {
            tag: "rust-v1.2.3".to_string(),
            ..release.clone()
        });
        assert_eq!(package.release_tag.as_deref(), Some("rust-v1.2.3"));
        assert_eq!(package.release_published_at, Some(release.published_at));
        assert!(package.version_tag_template.is_none());
    }

    #[test]
    fn version_tag_template_from_tag_keeps_prefix_and_suffix() {
        let version = Version::new(1, 2, 3, false);

        assert_eq!(
            Package::version_tag_template_from_tag("rust-v1.2.3", &version).as_deref(),
            Some("rust-v{}")
        );
        assert_eq!(
            Package::version_tag_template_from_tag("v1.2.3-beta.4", &version).as_deref(),
            Some("v{}-beta.4")
        );
    }

    #[test]
    fn version_tag_from_template_reconstructs_current_version() {
        let mut package = update_test_package(Version::new(1, 2, 3, false), Channel::Stable);
        package.version_tag_template = Some("rust-v{}-beta.4".to_string());

        assert_eq!(
            package.version_tag_from_template().as_deref(),
            Some("rust-v1.2.3-beta.4")
        );
    }

    #[test]
    fn datetime_tag_template_preserves_wrappers() {
        let version = Version::parse("20240203-110809-5046fc22").expect("datetime");
        let mut package = update_test_package(version.clone(), Channel::Stable);
        package.version_tag_template =
            Package::version_tag_template_from_tag("v20240203-110809-5046fc22-linux", &version);

        assert_eq!(package.version_tag_template.as_deref(), Some("v{}-linux"));
        assert_eq!(
            package.version_tag_from_template().as_deref(),
            Some("v20240203-110809-5046fc22-linux")
        );
    }
}
