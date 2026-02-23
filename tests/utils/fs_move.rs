use super::{is_cross_device, move_file_or_dir, move_file_or_dir_with_rename};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("upstream-fs-move-test-{name}-{nanos}"))
}

#[test]
fn move_file_or_dir_moves_file_with_rename_path() {
    let root = temp_root("rename");
    fs::create_dir_all(&root).expect("create root");
    let src = root.join("source.bin");
    let dst = root.join("dest.bin");
    fs::write(&src, b"content").expect("write source");

    move_file_or_dir(&src, &dst).expect("rename move");

    assert!(!src.exists());
    assert_eq!(fs::read(&dst).expect("read destination"), b"content");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn cross_device_error_detection_matches_error_kind() {
    let err = io::Error::new(io::ErrorKind::CrossesDevices, "cross-device");
    assert!(is_cross_device(&err));
}

#[test]
fn fallback_move_copies_and_removes_source_file() {
    let root = temp_root("fallback-file");
    fs::create_dir_all(&root).expect("create root");
    let src = root.join("source.txt");
    let dst = root.join("dest.txt");
    fs::write(&src, b"hello").expect("write source");

    move_file_or_dir_with_rename(&src, &dst, |_, _| {
        Err(io::Error::new(io::ErrorKind::CrossesDevices, "xdev"))
    })
    .expect("fallback move");

    assert!(!src.exists());
    assert_eq!(fs::read(&dst).expect("read destination"), b"hello");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn fallback_move_handles_directories_recursively() {
    let root = temp_root("fallback-dir");
    let src = root.join("src");
    let dst = root.join("dst");
    fs::create_dir_all(src.join("nested")).expect("create nested src");
    fs::write(src.join("nested/file.txt"), b"nested-data").expect("write nested file");

    move_file_or_dir_with_rename(&src, &dst, |_, _| {
        Err(io::Error::new(io::ErrorKind::CrossesDevices, "xdev"))
    })
    .expect("fallback dir move");

    assert!(!src.exists());
    assert_eq!(
        fs::read(dst.join("nested/file.txt")).expect("read moved file"),
        b"nested-data"
    );

    fs::remove_dir_all(root).expect("cleanup");
}
