use super::DesktopEntry;
use crate::models::common::enums::{Channel, Filetype, Provider};
use crate::models::upstream::Package;
use std::path::PathBuf;

#[test]
fn from_package_maps_name_exec_and_icon_paths() {
    let mut package = Package::with_defaults(
        "tool".to_string(),
        "owner/tool".to_string(),
        Filetype::Binary,
        None,
        None,
        Channel::Stable,
        Provider::Github,
        None,
    );
    package.exec_path = Some(PathBuf::from("/tmp/tool"));
    package.icon_path = Some(PathBuf::from("/tmp/tool.png"));

    let entry = DesktopEntry::from_package(&package);
    assert_eq!(entry.name.as_deref(), Some("tool"));
    assert_eq!(entry.exec.as_deref(), Some("/tmp/tool"));
    assert_eq!(entry.icon.as_deref(), Some("/tmp/tool.png"));
}

#[test]
fn from_package_uses_empty_icon_when_icon_path_missing() {
    let package = Package::with_defaults(
        "tool".to_string(),
        "owner/tool".to_string(),
        Filetype::Binary,
        None,
        None,
        Channel::Stable,
        Provider::Github,
        None,
    );

    let entry = DesktopEntry::from_package(&package);
    assert_eq!(entry.name.as_deref(), Some("tool"));
    assert_eq!(entry.exec, None);
    assert_eq!(entry.icon.as_deref(), Some(""));
}
