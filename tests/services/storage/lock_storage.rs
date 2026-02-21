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
