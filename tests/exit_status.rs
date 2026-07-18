use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "upstream-cli-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test home");
        Self { path }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_upstream"))
            .args(args)
            .env("HOME", &self.path)
            .env("XDG_CONFIG_HOME", self.path.join(".config"))
            .env("UPSTREAM_TEST_HOME", &self.path)
            .output()
            .expect("run upstream")
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn failed_remove_preview_returns_nonzero() {
    let home = TestHome::new("remove");
    let output = home.run(&["--no-pager", "remove", "missing", "--dry-run"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 failed"));
}

#[test]
fn failed_json_update_check_keeps_valid_stdout_and_returns_nonzero() {
    let home = TestHome::new("update-check");
    let output = home.run(&["--no-pager", "upgrade", "missing", "--check", "--json"]);

    assert!(!output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("structured stdout remains valid JSON");
    assert_eq!(value[0]["name"], "missing");
    assert_eq!(value[0]["state"], "not_installed");
}
