use super::{DoctorReport, expected_link_path, find_stale_symlink_names};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-doctor-test-{name}-{nanos}"))
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[test]
fn expected_link_path_uses_platform_naming() {
    let base = Path::new("/tmp/upstream-doctor");
    let link = expected_link_path(base, "tool");

    #[cfg(windows)]
    assert_eq!(link.file_name().and_then(|n| n.to_str()), Some("tool.exe"));

    #[cfg(not(windows))]
    assert_eq!(link.file_name().and_then(|n| n.to_str()), Some("tool"));
}

#[test]
fn find_stale_symlink_names_reports_orphans() {
    let root = temp_root("stale");
    fs::create_dir_all(&root).expect("create root");

    let installed = expected_link_path(&root, "installed");
    let orphan = expected_link_path(&root, "orphan");
    fs::write(&installed, b"x").expect("create installed link file");
    fs::write(&orphan, b"x").expect("create orphan link file");

    let installed_names = HashSet::from(["installed".to_string()]);
    let stale = find_stale_symlink_names(&root, &installed_names);
    assert_eq!(stale, vec!["orphan".to_string()]);

    cleanup(&root).expect("cleanup");
}

#[test]
fn doctor_report_hint_deduplicates_entries() {
    let mut report = DoctorReport::new();
    report.hint("Run upstream init");
    report.hint("Run upstream init");
    report.hint("Reinstall package");

    assert_eq!(report.hints.len(), 2);
    assert!(report.hints.contains(&"Run upstream init".to_string()));
    assert!(report.hints.contains(&"Reinstall package".to_string()));
}
