
use super::ShellManager;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-shell-test-{name}-{nanos}"))
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[cfg(unix)]
#[test]
fn add_to_paths_is_idempotent_and_escapes_special_characters() {
    let root = temp_root("add-idempotent");
    let install_path = root.join("tool\"dir$");
    let paths_file = root.join("paths.sh");
    fs::create_dir_all(&install_path).expect("create install dir");
    fs::write(&paths_file, "#!/usr/bin/env sh\n").expect("create paths file");
    let manager = ShellManager::new(&paths_file);

    manager.add_to_paths(&install_path).expect("first add");
    manager.add_to_paths(&install_path).expect("second add");

    let content = fs::read_to_string(&paths_file).expect("read paths file");
    assert_eq!(content.matches("export PATH=").count(), 1);
    assert!(content.contains("\\\""));
    assert!(content.contains("\\$"));

    cleanup(&root).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn remove_from_paths_removes_existing_export_line() {
    let root = temp_root("remove");
    let install_path = root.join("pkg/bin");
    let paths_file = root.join("paths.sh");
    fs::create_dir_all(&install_path).expect("create install dir");
    fs::write(&paths_file, "").expect("create paths file");
    let manager = ShellManager::new(&paths_file);

    manager.add_to_paths(&install_path).expect("add path");
    manager
        .remove_from_paths(&install_path)
        .expect("remove path");

    let content = fs::read_to_string(&paths_file).expect("read paths file");
    assert!(!content.contains("export PATH="));

    cleanup(&root).expect("cleanup");
}
