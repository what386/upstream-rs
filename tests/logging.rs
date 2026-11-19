#![cfg(feature = "testing_donotuseinrelease")]

use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

struct TestHome(PathBuf);

impl TestHome {
    fn new(config: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let home = Self(
            std::env::temp_dir().join(format!("upstream-tracing-{}-{nonce}", std::process::id())),
        );

        let config_dir = home.0.join(".config/upstream");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("config.toml"), config).unwrap();
        home
    }

    fn run(&self, args: &[&str], filter: Option<&str>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_upstream"));
        command
            .env("UPSTREAM_TEST_HOME", &self.0)
            .env_remove("RUST_LOG");
        if let Some(filter) = filter {
            command.env("RUST_LOG", filter);
        }

        command.arg("--no-pager").args(args).output().unwrap()
    }

    fn events(&self) -> Vec<Value> {
        fs::read_to_string(self.0.join(".upstream/upstream.log"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn successful_read_only_command_logs_outcome_without_changing_json_output() {
    let home = TestHome::new("");
    let result = home.run(&["list", "--json"], None);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    assert!(result.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        serde_json::json!([])
    );

    let events = home.events();
    assert!(
        events
            .iter()
            .any(|event| event["fields"]["message"] == "Command started")
    );

    let completed = events
        .iter()
        .find(|event| event["fields"]["outcome"] == "success")
        .unwrap();

    assert_eq!(completed["span"]["command"], "list");
    assert!(completed["fields"]["elapsed_ms"].is_u64());
    assert!(!events.iter().any(|event| event["level"] == "DEBUG"));
    assert!(!home.0.join(".upstream/log.jsonl").exists());
}

#[test]
fn debug_level_adds_diagnostics_and_error_filter_preserves_command_context() {
    let home = TestHome::new("[logging]\nlevel = 'debug'\n");
    assert!(home.run(&["list", "--json"], None).status.success());
    assert!(home.events().iter().any(|event| event["level"] == "DEBUG"));
    fs::remove_file(home.0.join(".upstream/upstream.log")).unwrap();
    let result = home.run(&["info", "missing-package", "--json"], Some("error"));
    assert!(!result.status.success());
    let events = home.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["fields"]["outcome"], "failure");
    assert_eq!(events[0]["span"]["command"], "info");
    assert!(events[0]["fields"]["error"].is_string());
}

#[test]
fn disabled_logging_and_early_requests_do_not_create_a_log() {
    let home = TestHome::new("[logging]\nenabled = false\n");
    assert!(
        home.run(&["list", "--json"], Some("trace"))
            .status
            .success()
    );

    assert!(!home.0.join(".upstream/upstream.log").exists());
    let home = TestHome::new("");
    assert!(home.run(&["--help"], None).status.success());
    assert!(
        home.run(&["__complete", "info", "0", "--", "missing"], None)
            .status
            .success()
    );

    assert!(!home.0.join(".upstream/upstream.log").exists());
    assert!(!home.run(&["history"], None).status.success());
}

#[test]
fn auth_commands_do_not_log_token_values() {
    let home = TestHome::new("[logging]\nlevel = 'debug'\n");
    let token = "tracing-test-private-token";
    assert!(
        home.run(&["auth", "set", &format!("github.api_token={token}")], None)
            .status
            .success()
    );

    let result = home.run(&["auth", "get", "github.api_token"], None);
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains(token));
    let log = fs::read_to_string(home.0.join(".upstream/upstream.log")).unwrap();
    assert!(!log.contains(token));
    assert!(log.contains("auth set"));
    assert!(log.contains("auth get"));
}

#[test]
fn invalid_configuration_still_logs_command_failure() {
    let home = TestHome::new("invalid toml");
    assert!(!home.run(&["list", "--json"], None).status.success());
    assert!(
        home.events()
            .iter()
            .any(|event| event["fields"]["outcome"] == "failure")
    );
}
