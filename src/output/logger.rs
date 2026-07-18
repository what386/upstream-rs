use crate::{
    application::operations::history_op::{HistoryRecord, LogLevel},
    models::upstream::config::{LoggingConfig, LoggingLevel},
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static LOGGER: OnceLock<Option<Logger>> = OnceLock::new();

struct Logger {
    file: Mutex<File>,
    path: PathBuf,
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
        let default_config = LoggerConfig::from(LoggingConfig::default());
        vacuum_file(path, default_config.vacuum, default_config.max_size_bytes);
        Ok(Self {
            file: Mutex::new(OpenOptions::new().create(true).append(true).open(path)?),
            path: path.to_path_buf(),
            config: Mutex::new(default_config),
        })
    }

    fn write(&self, record: HistoryRecord) {
        let Ok(config) = self.config.lock() else {
            return;
        };
        if !config.enabled || !level_enabled(record.level, config.level) {
            return;
        }
        let config = *config;
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

fn level_enabled(level: LogLevel, configured: LoggingLevel) -> bool {
    match (level, configured) {
        (LogLevel::Error, _) => true,
        (LogLevel::Warning, LoggingLevel::Error) => false,
        (LogLevel::Warning, _) => true,
        (LogLevel::Info, LoggingLevel::Info | LoggingLevel::Debug) => true,
        (LogLevel::Info, LoggingLevel::Error | LoggingLevel::Warn) => false,
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

pub fn write_operation(record: HistoryRecord) {
    with_logger(|logger| logger.write(record));
}

pub fn read_events(path: &Path) -> std::io::Result<Vec<HistoryRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(content
        .lines()
        .filter_map(|line| serde_json::from_str::<HistoryRecord>(line).ok())
        .filter(|record| record.record_type == "operation")
        .collect())
}

pub fn warning(message: impl Into<String>) {
    crate::application::operations::history_op::record_item(
        None,
        LogLevel::Warning,
        crate::application::operations::history_op::Outcome::Success,
        message,
    );
}

pub fn status(subject: impl Into<String>, status: impl Into<String>, message: impl Into<String>) {
    use crate::application::operations::history_op::{LogLevel, Outcome};
    let status = status.into();
    let (level, outcome) = match status.as_str() {
        "fail" => (LogLevel::Error, Outcome::Failure),
        "warn" => (LogLevel::Warning, Outcome::Success),
        _ => (LogLevel::Info, Outcome::Success),
    };
    warning_or_status(Some(subject.into()), level, outcome, message.into());
}

fn warning_or_status(
    package: Option<String>,
    level: LogLevel,
    outcome: crate::application::operations::history_op::Outcome,
    message: String,
) {
    crate::application::operations::history_op::record_item(package, level, outcome, message);
}

pub fn error(message: impl Into<String>) {
    use crate::application::operations::history_op::{LogLevel, Outcome};
    warning_or_status(None, LogLevel::Error, Outcome::Failure, message.into());
}

#[cfg(test)]
mod tests {
    use super::vacuum_file;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn vacuum_keeps_only_the_newest_records() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("upstream-log-{nonce}.jsonl"));
        fs::write(&path, "one\ntwo\nthree\n").expect("write log");
        vacuum_file(&path, 2, 0);
        assert_eq!(fs::read_to_string(&path).expect("read log"), "two\nthree\n");
        fs::remove_file(path).expect("cleanup log");
    }
}
