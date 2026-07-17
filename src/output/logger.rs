use crate::models::upstream::config::{LoggingConfig, LoggingLevel};
use chrono::Utc;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

impl LoggingLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

#[derive(Debug, Serialize)]
struct LogEvent {
    timestamp: String,
    event: String,
    level: String,
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
    config: Mutex<LoggerConfig>,
}

#[derive(Clone, Copy)]
struct LoggerConfig {
    enabled: bool,
    level: LoggingLevel,
    vacuum: usize,
    max_size_bytes: u64,
}

impl From<LoggingConfig> for LoggerConfig {
    fn from(config: LoggingConfig) -> Self {
        Self {
            enabled: config.enabled,
            level: config.level,
            vacuum: config.vacuum,
            max_size_bytes: config.max_size_bytes(),
        }
    }
}

impl Logger {
    fn new(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        vacuum_file(path, 10_000, 10 * 1024 * 1024);

        Ok(Self {
            file: Mutex::new(OpenOptions::new().create(true).append(true).open(path)?),
            path: path.to_path_buf(),
            command: Mutex::new(None),
            config: Mutex::new(LoggerConfig {
                enabled: true,
                level: LoggingLevel::Info,
                vacuum: 10_000,
                max_size_bytes: 10 * 1024 * 1024,
            }),
        })
    }

    fn write(
        &self,
        event: impl Into<String>,
        level: LoggingLevel,
        subject: Option<String>,
        status: Option<String>,
        message: Option<String>,
        success: Option<bool>,
    ) {
        let Ok(config) = self.config.lock() else {
            return;
        };
        if !config.enabled || level > config.level {
            return;
        }
        let config = *config;

        let command = self.command.lock().ok().and_then(|command| command.clone());
        let record = LogEvent {
            timestamp: Utc::now().to_rfc3339(),
            event: event.into(),
            level: level.as_str().to_string(),
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
        let oversized = config.max_size_bytes > 0
            && file
                .metadata()
                .is_ok_and(|metadata| metadata.len() > config.max_size_bytes);
        drop(file);
        if oversized {
            vacuum_file(&self.path, config.vacuum, config.max_size_bytes);
        }
    }
}

fn with_logger(action: impl FnOnce(&Logger)) {
    if let Some(Some(logger)) = LOGGER.get() {
        action(logger);
    }
}

fn vacuum_file(path: &Path, limit: usize, max_size_bytes: u64) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut first = if limit == 0 {
        lines.len()
    } else {
        lines.len().saturating_sub(limit)
    };
    while first < lines.len() && max_size_bytes > 0 {
        let bytes = lines[first..]
            .iter()
            .map(|line| line.len() + 1)
            .sum::<usize>();
        if bytes as u64 <= max_size_bytes {
            break;
        }
        first += 1;
    }
    let mut output = lines[first..].join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    let _ = fs::write(path, output);
}

pub fn init(path: &Path) {
    let _ = LOGGER.set(Logger::new(path).ok());
}

pub fn configure(config: LoggingConfig) {
    with_logger(|logger| {
        let config = LoggerConfig::from(config);
        if config.enabled {
            vacuum_file(&logger.path, config.vacuum, config.max_size_bytes);
        }
        if let Ok(mut current) = logger.config.lock() {
            *current = config;
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
    with_logger(|logger| {
        logger.write(
            "warning",
            LoggingLevel::Warn,
            None,
            None,
            Some(message.into()),
            None,
        )
    });
}

pub fn status(subject: impl Into<String>, status: impl Into<String>, message: impl Into<String>) {
    let status = status.into();
    let level = match status.as_str() {
        "fail" => LoggingLevel::Error,
        "warn" => LoggingLevel::Warn,
        _ => LoggingLevel::Info,
    };
    with_logger(|logger| {
        logger.write(
            "status",
            level,
            Some(subject.into()),
            Some(status),
            Some(message.into()),
            None,
        )
    });
}

pub fn error(message: impl Into<String>) {
    with_logger(|logger| {
        logger.write(
            "error",
            LoggingLevel::Error,
            None,
            None,
            Some(message.into()),
            None,
        )
    });
}

pub fn command_result(success: bool, message: Option<String>) {
    with_logger(|logger| {
        logger.write(
            "command_finished",
            if success {
                LoggingLevel::Info
            } else {
                LoggingLevel::Error
            },
            None,
            None,
            message,
            Some(success),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::{LogEvent, vacuum_file};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn serializes_one_json_record_without_optional_fields() {
        let event = LogEvent {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            event: "warning".to_string(),
            level: "warn".to_string(),
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

        vacuum_file(&path, 2, 0);

        assert_eq!(fs::read_to_string(&path).expect("read log"), "two\nthree\n");
        fs::remove_file(path).expect("cleanup log");
    }
}
