use super::LockStorage;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_lock_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("upstream-lock-test-{name}-{nanos}"))
            .join("metadata")
            .join("lock")
    }

    #[test]
    fn lock_prevents_concurrent_acquire() {
        let lock_path = unique_lock_path("concurrent");
        let _guard = LockStorage::acquire_at(&lock_path, "test").expect("first lock");

        let err = LockStorage::acquire_at(&lock_path, "test").expect_err("must fail");
        assert!(err.to_string().contains("already running"));

        let _ = fs::remove_dir_all(lock_path.parent().unwrap().parent().unwrap());
    }

    #[test]
fn lock_releases_on_drop() {
        let lock_path = unique_lock_path("release");
        {
            let _guard = LockStorage::acquire_at(&lock_path, "test").expect("first lock");
        }

        let _guard2 = LockStorage::acquire_at(&lock_path, "test").expect("lock after drop");

    let _ = fs::remove_dir_all(lock_path.parent().unwrap().parent().unwrap());
}

#[test]
fn stale_lock_is_recovered_automatically() {
    let lock_path = unique_lock_path("stale-recover");
    fs::create_dir_all(lock_path.parent().expect("lock parent")).expect("create lock parent");
    // Deliberately invalid/non-existent pid with old start time.
    fs::write(
        &lock_path,
        "pid=999999\noperation=test\nstarted_at_unix=1\n",
    )
    .expect("write stale lock");

    let _guard = LockStorage::acquire_at(&lock_path, "new-op").expect("recover stale lock");
    let contents = fs::read_to_string(&lock_path).expect("read lock");
    assert!(contents.contains("operation=new-op"));

    let _ = fs::remove_dir_all(lock_path.parent().unwrap().parent().unwrap());
}

#[test]
fn parse_lock_metadata_extracts_known_fields() {
    let meta = LockStorage::parse_lock_metadata(
        "pid=123\noperation=upgrade\nstarted_at_unix=456\nunknown=ignored\n",
    );
    assert_eq!(meta.pid, Some(123));
    assert_eq!(meta.operation.as_deref(), Some("upgrade"));
    assert_eq!(meta.started_at_unix, Some(456));
}

#[test]
fn active_lock_still_blocks_second_acquire() {
    let lock_path = unique_lock_path("active-block");
    fs::create_dir_all(lock_path.parent().expect("lock parent")).expect("create lock parent");
    let current_pid = std::process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    fs::write(
        &lock_path,
        format!("pid={current_pid}\noperation=test\nstarted_at_unix={now}\n"),
    )
    .expect("write active lock");

    let err = LockStorage::acquire_at(&lock_path, "next-op").expect_err("must block");
    assert!(err.to_string().contains("already running"));

    let _ = fs::remove_dir_all(lock_path.parent().unwrap().parent().unwrap());
}
