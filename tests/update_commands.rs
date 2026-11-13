//! Exercise the real startup path in an isolated home, with unusable normal startup state.
#![cfg(unix)]

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

struct Home(PathBuf);

impl Home {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "upstream-update-cli-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn cache(&self) -> PathBuf {
        self.0.join(".upstream/cache/updates/available.json")
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_upstream"))
            .args(args)
            .env("HOME", &self.0)
            .env("UPSTREAM_TEST_HOME", &self.0)
            .env("XDG_CONFIG_HOME", self.0.join(".config"))
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("HTTPS_PROXY", "http://127.0.0.1:1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!("startup command timed out: {output:?}");
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        child.wait_with_output().unwrap()
    }

    fn write_cache(&self, bytes: &[u8]) {
        fs::create_dir_all(self.cache().parent().unwrap()).unwrap();
        fs::write(self.cache(), bytes).unwrap();
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot() -> serde_json::Value {
    serde_json::json!({"schema_version": 1, "updates": {
        "github:owner/tool": {
            "current": "1.0.0", "latest": "2.0.0", "provider": "github", "channel": "Stable",
            "checked_at": "2020-01-01T00:00:00Z", "installed_state": {}
        }
    }})
}

#[test]
fn notify_is_silent_on_first_run_and_creates_nothing() {
    let home = Home::new();
    let output = home.run(&["updates", "--notify"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read_dir(&home.0).unwrap().count(), 0);
}

#[test]
fn cached_views_distinguish_missing_and_empty_and_reject_mixed_output_modes() {
    let home = Home::new();
    let output = home.run(&["updates"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No cached update results")
    );

    home.write_cache(br#"{"schema_version":1,"updates":{}}"#);
    let output = home.run(&["updates"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No cached available updates")
    );

    let output = home.run(&["updates", "--notify"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(
        !home
            .run(&["updates", "--notify", "--json"])
            .status
            .success()
    );

    assert!(!home.run(&["notify"]).status.success());
}

#[test]
fn readers_bypass_config_migrations_database_and_occupied_lock() {
    let home = Home::new();
    let original = serde_json::to_vec(&snapshot()).unwrap();
    home.write_cache(&original);
    let data = home.0.join(".upstream");
    fs::write(data.join("config.toml"), b"this is not TOML").unwrap();
    fs::create_dir_all(data.join("metadata")).unwrap();
    fs::write(data.join("metadata/packages.db"), b"not a database").unwrap();
    fs::create_dir_all(data.join("state")).unwrap();
    let lock = format!("pid={}\noperation=test\n", std::process::id());
    fs::write(data.join("state/lock"), &lock).unwrap();
    let before = walkdir::WalkDir::new(&home.0).into_iter().count();
    let output = home.run(&["updates", "--notify"]);
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    let message = String::from_utf8(output.stdout).unwrap();
    assert!(message.contains("1 pending software update (last checked "));
    assert!(!message.contains("oldest check"));
    assert!(message.contains("Run 'upstream updates' to view them."));
    let output = home.run(&["updates", "--json"]);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        snapshot()
    );

    assert!(output.stderr.is_empty());
    let output = home.run(&["updates"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("github:owner/tool")
    );

    assert_eq!(fs::read_to_string(data.join("state/lock")).unwrap(), lock);
    assert_eq!(fs::read(home.cache()).unwrap(), original);
    assert_eq!(walkdir::WalkDir::new(&home.0).into_iter().count(), before);
}

#[test]
fn notify_tolerates_unusable_caches_while_updates_explains_errors() {
    let home = Home::new();
    for bytes in [
        b"broken".as_slice(),
        br#"{"schema_version":2,"updates":{}}"#,
    ] {
        home.write_cache(bytes);
        let output = home.run(&["updates", "--notify"]);
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        let output = home.run(&["updates"]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("upstream upgrade --check")
        );
    }

    fs::remove_file(home.cache()).unwrap();
    fs::create_dir(home.cache()).unwrap();
    let output = home.run(&["updates", "--notify"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
