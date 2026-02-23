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
    assert!(package.install_path.is_none());
    assert!(package.exec_path.is_none());
    assert_eq!(package.version.to_string(), "0.0.0");
}

#[test]
fn from_package_round_trips_reference_fields() {
    let package = Package::with_defaults(
        "ripgrep".to_string(),
        "BurntSushi/ripgrep".to_string(),
        Filetype::Binary,
        Some("linux".to_string()),
        Some("symbols".to_string()),
        Channel::Preview,
        Provider::Github,
        None,
    );

    let reference = PackageReference::from_package(package);
    assert_eq!(reference.name, "ripgrep");
    assert_eq!(reference.repo_slug, "BurntSushi/ripgrep");
    assert_eq!(reference.filetype, Filetype::Binary);
    assert_eq!(reference.channel, Channel::Preview);
    assert_eq!(reference.provider, Provider::Github);
    assert_eq!(reference.match_pattern.as_deref(), Some("linux"));
    assert_eq!(reference.exclude_pattern.as_deref(), Some("symbols"));
}
