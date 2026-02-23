use super::PackageUpgrader;
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-upgrader-test-{name}-{nanos}"))
}

fn cleanup(path: &Path) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[test]
fn backup_path_appends_old_suffix() {
    let original = Path::new("/tmp/example/tool");
    let backup = PackageUpgrader::backup_path(original).expect("backup path");
    assert!(backup.ends_with("tool.old"));
}

#[test]
fn remove_path_if_exists_handles_files_and_directories() {
    let root = temp_root("remove");
    let file = root.join("f.bin");
    let dir = root.join("d");
    fs::create_dir_all(&dir).expect("create dir");
    fs::write(&file, b"content").expect("write file");

    PackageUpgrader::remove_path_if_exists(&file).expect("remove file");
    PackageUpgrader::remove_path_if_exists(&dir).expect("remove dir");
    PackageUpgrader::remove_path_if_exists(&root.join("missing")).expect("ignore missing");

    assert!(!file.exists());
    assert!(!dir.exists());

    cleanup(&root).expect("cleanup");
}
