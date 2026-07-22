use crate::models::common::enums::{Channel, Filetype, Provider, TrustMode};
use crate::models::upstream::{InstallType, Package};
use crate::providers::pattern_matcher::PatternTable;
use serde::{Deserialize, Serialize};

/// The bare minimum needed to install a package. Essentially the args to
/// `Package::with_defaults` — no install state and no paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReference {
    pub name: String,
    pub repo_slug: String,
    pub filetype: Filetype,
    pub channel: Channel,
    pub provider: Provider,
    pub base_url: Option<String>,
    pub install_type: InstallType,
    pub version_tag: Option<String>,
    pub build_branch: Option<String>,
    pub build_commit: Option<String>,
    #[serde(default)]
    pub match_pattern: PatternTable,
    #[serde(default)]
    pub exclude_pattern: PatternTable,
    #[serde(default)]
    pub trust_mode: Option<TrustMode>,
}

impl PackageReference {
    pub fn into_package(self) -> Package {
        let mut package = Package::with_defaults(
            self.name,
            self.repo_slug,
            self.filetype,
            None,
            None,
            self.channel,
            self.provider,
            self.base_url,
        );
        package.install_type = self.install_type;
        package.build_branch = self.build_branch;
        package.build_commit = self.build_commit;
        package.match_pattern = self.match_pattern;
        package.exclude_pattern = self.exclude_pattern;
        package
    }

    pub fn from_package(package: Package) -> Self {
        let version_tag = release_version_tag(&package);
        Self {
            name: package.name,
            repo_slug: package.repo_slug,
            filetype: package.filetype,
            channel: package.channel,
            provider: package.provider,
            base_url: package.base_url,
            install_type: package.install_type.clone(),
            version_tag,
            build_branch: package.build_branch,
            build_commit: package.build_commit,
            match_pattern: package.match_pattern,
            exclude_pattern: package.exclude_pattern,
            trust_mode: None,
        }
    }
}

fn release_version_tag(package: &Package) -> Option<String> {
    if package.version.is_unknown()
        || (package.install_type == InstallType::Build && package.build_branch.is_some())
    {
        return None;
    }

    package.installed_release_tag()
}

#[cfg(test)]
mod tests {
    use super::PackageReference;
    use crate::models::common::{
        Version,
        enums::{Channel, Filetype, Provider},
    };
    use crate::models::upstream::{InstallType, Package};
    use crate::providers::pattern_matcher::PatternTable;

    fn reference() -> PackageReference {
        PackageReference {
            name: "fd".to_string(),
            repo_slug: "sharkdp/fd".to_string(),
            filetype: Filetype::Archive,
            channel: Channel::Stable,
            provider: Provider::Github,
            base_url: Some("https://api.github.com".to_string()),
            install_type: InstallType::Build,
            version_tag: None,
            build_branch: Some("main".to_string()),
            build_commit: Some("abcdef123456".to_string()),
            match_pattern: PatternTable::from_patterns(["x86_64"]),
            exclude_pattern: PatternTable::from_patterns(["debug"]),
            trust_mode: None,
        }
    }

    #[test]
    fn into_package_keeps_install_inputs_and_applies_runtime_defaults() {
        let package = reference().into_package();

        assert_eq!(package.name, "fd");
        assert_eq!(package.repo_slug, "sharkdp/fd");
        assert_eq!(package.filetype, Filetype::Archive);
        assert_eq!(package.channel, Channel::Stable);
        assert_eq!(package.provider, Provider::Github);
        assert_eq!(package.base_url.as_deref(), Some("https://api.github.com"));
        assert_eq!(package.install_type, InstallType::Build);
        assert_eq!(package.build_branch.as_deref(), Some("main"));
        assert_eq!(package.build_commit.as_deref(), Some("abcdef123456"));
        assert!(package.install_path.is_none());
        assert!(package.exec_path.is_none());
        assert_eq!(package.version.to_string(), "0.0.0");
    }

    #[test]
    fn from_package_round_trips_reference_fields() {
        let mut package = Package::with_defaults(
            "ripgrep".to_string(),
            "BurntSushi/ripgrep".to_string(),
            Filetype::Binary,
            Some("linux".to_string()),
            Some("symbols".to_string()),
            Channel::Preview,
            Provider::Github,
            None,
        );
        package.version = Version::new(1, 2, 3, false);
        package.version_tag_template = Some("rust-v{}-beta.4".to_string());
        package.build_branch = Some("dev".to_string());
        package.build_commit = Some("0123456789abcdef".to_string());

        let reference = PackageReference::from_package(package);
        assert_eq!(reference.name, "ripgrep");
        assert_eq!(reference.repo_slug, "BurntSushi/ripgrep");
        assert_eq!(reference.filetype, Filetype::Binary);
        assert_eq!(reference.channel, Channel::Preview);
        assert_eq!(reference.provider, Provider::Github);
        assert_eq!(reference.install_type, InstallType::Release);
        assert_eq!(reference.version_tag.as_deref(), Some("rust-v1.2.3-beta.4"));
        assert_eq!(reference.build_branch.as_deref(), Some("dev"));
        assert_eq!(reference.build_commit.as_deref(), Some("0123456789abcdef"));
        assert_eq!(reference.match_pattern.to_string(), "linux");
        assert_eq!(reference.exclude_pattern.to_string(), "symbols");
    }

    #[test]
    fn from_package_keeps_release_tag_for_release_backed_builds() {
        let mut package = reference().into_package();
        package.build_branch = None;
        package.version = Version::new(1, 2, 3, false);
        package.version_tag_template = Some("v{}".to_string());

        let reference = PackageReference::from_package(package);

        assert_eq!(reference.version_tag.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn from_package_prefers_exact_release_tag_over_legacy_template() {
        let mut package = reference().into_package();
        package.build_branch = None;
        package.version = Version::new(1, 2, 3, false);
        package.release_tag = Some("custom-release-name".to_string());
        package.version_tag_template = Some("v{}".to_string());

        let reference = PackageReference::from_package(package);

        assert_eq!(
            reference.version_tag.as_deref(),
            Some("custom-release-name")
        );
    }
}
