use super::SymlinkManager;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-symlink-test-{name}-{nanos}"))
}

fn cleanup(path: &PathBuf) -> io::Result<()> {
    fs::remove_dir_all(path)
}

#[cfg(unix)]
#[test]
fn add_link_replaces_dangling_symlink() {
    let root = temp_root("replace-dangling");
    let symlinks_dir = root.join("symlinks");
    let missing_target = root.join("missing-target");
    let new_target = root.join("new-target");
    let link_name = "arduino";
    let link_path = symlinks_dir.join(link_name);

    fs::create_dir_all(&symlinks_dir).expect("create symlink dir");
    fs::write(&new_target, b"new-target").expect("write new target");
    std::os::unix::fs::symlink(&missing_target, &link_path).expect("create dangling symlink");
    assert!(!link_path.exists(), "dangling symlink should not exist via exists()");
    assert!(
        fs::symlink_metadata(&link_path).is_ok(),
        "dangling symlink should still be present on disk"
    );

    let manager = SymlinkManager::new(&symlinks_dir);
    manager
        .add_link(&new_target, link_name)
        .expect("replace dangling symlink");

    let target = fs::read_link(&link_path).expect("read link target");
    assert_eq!(target, new_target);

    cleanup(&root).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn remove_link_removes_dangling_symlink() {
    let root = temp_root("remove-dangling");
    let symlinks_dir = root.join("symlinks");
    let missing_target = root.join("missing-target");
    let link_name = "arduino";
    let link_path = symlinks_dir.join(link_name);

    fs::create_dir_all(&symlinks_dir).expect("create symlink dir");
    std::os::unix::fs::symlink(&missing_target, &link_path).expect("create dangling symlink");
    assert!(
        fs::symlink_metadata(&link_path).is_ok(),
        "dangling symlink should be present before removal"
    );

    let manager = SymlinkManager::new(&symlinks_dir);
    manager
        .remove_link(link_name)
        .expect("remove dangling symlink");

    assert!(
        fs::symlink_metadata(&link_path).is_err(),
        "dangling symlink should be removed"
    );

    cleanup(&root).expect("cleanup");
}
