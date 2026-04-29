use crate::models::{
    common::enums::{Channel, Filetype, Provider},
    upstream::Package,
};
use serde::{Deserialize, Serialize};

/// The bare minimum needed to install a package. Essentially the args to
/// `Package::with_defaults` — no install state, no paths, no version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageReference {
    pub name: String,
    pub repo_slug: String,
    pub filetype: Filetype,
    pub channel: Channel,
    pub provider: Provider,
    pub base_url: Option<String>,
    #[serde(default)]
    pub build_branch: Option<String>,
    #[serde(default)]
    pub build_commit: Option<String>,
    pub match_pattern: Option<String>,
    pub exclude_pattern: Option<String>,
}

impl PackageReference {
    pub fn into_package(self) -> Package {
        let mut package = Package::with_defaults(
            self.name,
            self.repo_slug,
            self.filetype,
            self.match_pattern,
            self.exclude_pattern,
            self.channel,
            self.provider,
            self.base_url,
        );
        package.build_branch = self.build_branch;
        package.build_commit = self.build_commit;
        package
    }

    pub fn from_package(package: Package) -> Self {
        Self {
            name: package.name,
            repo_slug: package.repo_slug,
            filetype: package.filetype,
            channel: package.channel,
            provider: package.provider,
            base_url: package.base_url,
            build_branch: package.build_branch,
            build_commit: package.build_commit,
            match_pattern: package.match_pattern,
            exclude_pattern: package.exclude_pattern,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PackageReference;
    use crate::models::common::enums::{Channel, Filetype, Provider};
    use crate::models::upstream::Package;

    fn reference() -> PackageReference {
        PackageReference {
            name: "fd".to_string(),
            repo_slug: "sharkdp/fd".to_string(),
            filetype: Filetype::Archive,
            channel: Channel::Stable,
            provider: Provider::Github,
            base_url: Some("https://api.github.com".to_string()),
            build_branch: Some("main".to_string()),
            build_commit: Some("abcdef123456".to_string()),
            match_pattern: Some("x86_64".to_string()),
            exclude_pattern: Some("debug".to_string()),
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
        package.build_branch = Some("dev".to_string());
        package.build_commit = Some("0123456789abcdef".to_string());

        let reference = PackageReference::from_package(package);
        assert_eq!(reference.name, "ripgrep");
        assert_eq!(reference.repo_slug, "BurntSushi/ripgrep");
        assert_eq!(reference.filetype, Filetype::Binary);
        assert_eq!(reference.channel, Channel::Preview);
        assert_eq!(reference.provider, Provider::Github);
        assert_eq!(reference.build_branch.as_deref(), Some("dev"));
        assert_eq!(reference.build_commit.as_deref(), Some("0123456789abcdef"));
        assert_eq!(reference.match_pattern.as_deref(), Some("linux"));
        assert_eq!(reference.exclude_pattern.as_deref(), Some("symbols"));
    }

    #[test]
    fn deserializes_without_branch_fields_for_legacy_manifests() {
        let legacy = r#"{
            "name":"tool",
            "repo_slug":"owner/tool",
            "filetype":"Binary",
            "channel":"Stable",
            "provider":"Github",
            "base_url":null,
            "match_pattern":null,
            "exclude_pattern":null
        }"#;
        let reference: PackageReference =
            serde_json::from_str(legacy).expect("deserialize legacy reference");
        assert!(reference.build_branch.is_none());
        assert!(reference.build_commit.is_none());
    }
}
