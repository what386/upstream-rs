use chrono::Utc;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug, Serialize)]
struct LogEvent {
    timestamp: String,
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    success: Option<bool>,
}

static LOGGER: OnceLock<Option<Logger>> = OnceLock::new();

struct Logger {
    file: Mutex<File>,
    path: PathBuf,
    command: Mutex<Option<String>>,
    enabled: AtomicBool,
}

impl Logger {
    fn new(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        vacuum_file(path, 200);

        Ok(Self {
            file: Mutex::new(OpenOptions::new().create(true).append(true).open(path)?),
            path: path.to_path_buf(),
            command: Mutex::new(None),
            enabled: AtomicBool::new(true),
        })
    }

    fn write(
        &self,
        event: impl Into<String>,
        subject: Option<String>,
        status: Option<String>,
        message: Option<String>,
        success: Option<bool>,
    ) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let command = self.command.lock().ok().and_then(|command| command.clone());
        let record = LogEvent {
            timestamp: Utc::now().to_rfc3339(),
            event: event.into(),
            command,
            subject,
            status,
            message,
            success,
        };

        let Ok(line) = serde_json::to_string(&record) else {
            return;
        };
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        let _ = writeln!(file, "{line}");
    }
}

fn with_logger(action: impl FnOnce(&Logger)) {
    if let Some(Some(logger)) = LOGGER.get() {
        action(logger);
    }
}

fn vacuum_file(path: &Path, limit: usize) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };

    let lines: Vec<&str> = content.lines().collect();
    let retained = if limit == 0 {
        &lines[lines.len()..]
    } else {
        &lines[lines.len().saturating_sub(limit)..]
    };
    let mut output = retained.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    let _ = fs::write(path, output);
}

pub fn init(path: &Path) {
    let _ = LOGGER.set(Logger::new(path).ok());
}

pub fn configure(enabled: bool, vacuum: usize) {
    with_logger(|logger| {
        logger.enabled.store(enabled, Ordering::Relaxed);
        if enabled {
            vacuum_file(&logger.path, vacuum);
        }
    });
}

pub fn set_command(command: impl Into<String>) {
    with_logger(|logger| {
        if let Ok(mut current) = logger.command.lock() {
            *current = Some(command.into());
        }
    });
}

pub fn warning(message: impl Into<String>) {
    with_logger(|logger| logger.write("warning", None, None, Some(message.into()), None));
}

pub fn status(subject: impl Into<String>, status: impl Into<String>, message: impl Into<String>) {
    with_logger(|logger| {
        logger.write(
            "status",
            Some(subject.into()),
            Some(status.into()),
            Some(message.into()),
            None,
        )
    });
}

pub fn error(message: impl Into<String>) {
    with_logger(|logger| logger.write("error", None, None, Some(message.into()), None));
}

pub fn command_result(success: bool, message: Option<String>) {
    with_logger(|logger| logger.write("command_finished", None, None, message, Some(success)));
}

#[cfg(test)]
mod tests {
    use super::{LogEvent, vacuum_file};
    use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn serializes_one_json_record_without_optional_fields() {
        let event = LogEvent {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            event: "warning".to_string(),
            command: None,
            subject: None,
            status: None,
            message: Some("careful".to_string()),
            success: None,
        };

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&event).expect("serialize event"))
                .expect("valid JSON");
        assert_eq!(value["event"], "warning");
        assert_eq!(value["message"], "careful");
        assert!(value.get("command").is_none());
    }

    #[test]
    fn vacuum_keeps_only_the_newest_records() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = PathBuf::from(std::env::temp_dir()).join(format!("upstream-log-{nonce}.jsonl"));
        fs::write(&path, "one\ntwo\nthree\n").expect("write log");

        vacuum_file(&path, 2);

        assert_eq!(fs::read_to_string(&path).expect("read log"), "two\nthree\n");
        fs::remove_file(path).expect("cleanup log");
    }
}
