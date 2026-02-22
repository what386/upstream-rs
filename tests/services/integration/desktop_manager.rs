use super::DesktopManager;
use crate::models::common::DesktopEntry;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-desktop-manager-test-{name}-{nanos}"))
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[test]
fn parse_desktop_file_preserves_localized_and_extra_fields() {
    let root = temp_root("parse");
    fs::create_dir_all(&root).expect("create temp root");
    let desktop_file = root.join("app.desktop");

    fs::write(
        &desktop_file,
        r#"
Name=ignored-outside-section
[Desktop Entry]
Name=KDE Connect
Name[fr]=KDEConnect
GenericName=Device Synchronization
Comment=Make all your devices one
Exec=kdeconnect-app
Icon=kdeconnect
Type=Application
Terminal=false
Categories=Qt;KDE;Network
X-AppImage-Name=KDE_Connect

[Desktop Action New]
Name=ignored-action
"#,
    )
    .expect("write desktop file");

    let entry = DesktopManager::parse_desktop_file(&desktop_file).expect("parse desktop file");

    assert_eq!(entry.name.as_deref(), Some("KDE Connect"));
    assert_eq!(entry.comment.as_deref(), Some("Make all your devices one"));
    assert_eq!(entry.exec.as_deref(), Some("kdeconnect-app"));
    assert_eq!(entry.icon.as_deref(), Some("kdeconnect"));
    assert_eq!(entry.categories.as_deref(), Some("Qt;KDE;Network"));
    assert!(!entry.terminal);

    assert_eq!(
        entry.extras.get("Name[fr]").map(String::as_str),
        Some("KDEConnect")
    );
    assert_eq!(
        entry.extras.get("GenericName").map(String::as_str),
        Some("Device Synchronization")
    );
    assert_eq!(
        entry.extras.get("X-AppImage-Name").map(String::as_str),
        Some("KDE_Connect")
    );

    cleanup(&root).expect("cleanup");
}

#[test]
fn ensure_name_prefers_localized_then_fallback() {
    let mut localized_only = DesktopEntry::default();
    localized_only.set_field("Name[en_GB]", "Localized App".to_string());

    let localized_resolved = localized_only.ensure_name("fallback-name");
    assert_eq!(localized_resolved.name.as_deref(), Some("Localized App"));

    let fallback_resolved = DesktopEntry::default().ensure_name("fallback-name");
    assert_eq!(fallback_resolved.name.as_deref(), Some("fallback-name"));
}

#[test]
fn serialize_preserves_extras_and_sanitize_overrides_exec_icon_terminal() {
    let mut entry = DesktopEntry::default();
    entry.set_field("Name[en_GB]", "Localized App".to_string());
    entry.set_field("X-AppImage-Version", "25.12.2-1".to_string());
    entry.set_field("Exec", "embedded-exec".to_string());
    entry.set_field("Icon", "embedded-icon".to_string());
    entry.set_field("Terminal", "true".to_string());

    let rendered = entry
        .ensure_name("fallback-name")
        .sanitize(Path::new("/tmp/upstream-bin"), None)
        .to_desktop_file();

    assert!(rendered.contains("Name=Localized App\n"));
    assert!(rendered.contains("Exec=/tmp/upstream-bin\n"));
    assert!(rendered.contains("Icon=\n"));
    assert!(rendered.contains("Terminal=false\n"));
    assert!(rendered.contains("Name[en_GB]=Localized App\n"));
    assert!(rendered.contains("X-AppImage-Version=25.12.2-1\n"));
}
