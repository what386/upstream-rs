//! File diagnostics for the CLI. Library callers choose their own subscriber.
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    backtrace::Backtrace,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    panic::{self, PanicHookInfo},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, Once},
};
use tracing_subscriber::{EnvFilter, fmt::MakeWriter};

use crate::models::upstream::config::{LoggingConfig, LoggingLevel};

pub const LOG_FILE: &str = "upstream.log";

/// Remove credentials and signed query parameters before recording a source URL.
pub(crate) fn source_url(value: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(value) else {
        return "<invalid URL>".to_owned();
    };

    let _ = url.set_password(None);
    let _ = url.set_username("");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

/// Read only logging settings before migrations normalize legacy configuration.
/// Missing configuration uses defaults, without creating a config file.
pub fn read_config(path: &Path) -> Result<LoggingConfig> {
    #[derive(Default, Deserialize)]
    #[serde(default)]
    struct Settings {
        logging: LoggingConfig,
    }

    match fs::read_to_string(path) {
        Ok(content) => Ok(toml::from_str::<Settings>(&content)
            .context("Failed to read logging configuration")?
            .logging),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(LoggingConfig::default()),
        Err(error) => Err(error).context("Failed to read config file"),
    }
}

pub fn init(path: &Path, config: LoggingConfig) -> Result<()> {
    install_panic_hook(path.parent().unwrap_or_else(|| Path::new(".")));
    if !config.enabled {
        return Ok(());
    }

    let filter = filter(config.level)?;
    let writer = LogWriter::new(path, config).context("Failed to open diagnostic log")?;
    tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_env_filter(filter)
        .with_writer(writer)
        .try_init()
        .map_err(|error| anyhow::anyhow!("Failed to initialize logging: {error}"))
}

fn install_panic_hook(directory: &Path) {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let directory = directory.to_owned();
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            // Write directly: a panic may occur while the tracing writer is locked.
            if let Ok(path) = write_crash_report(&directory, info) {
                let _ = writeln!(
                    io::stderr().lock(),
                    "Crash report written to {}",
                    path.display()
                );
            }

            previous(info);
        }));
    });
}

fn write_crash_report(directory: &Path, info: &PanicHookInfo<'_>) -> io::Result<PathBuf> {
    let timestamp = chrono::Utc::now();
    let path = directory.join(format!(
        "crash-{}.txt",
        timestamp.format("%Y%m%dT%H%M%S%.9fZ")
    ));

    fs::create_dir_all(directory)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(&path)?;
    let thread = std::thread::current();
    writeln!(file, "Upstream version: {}", env!("CARGO_PKG_VERSION"))?;
    writeln!(file, "Timestamp: {}", timestamp.to_rfc3339())?;
    writeln!(file, "OS: {}", std::env::consts::OS)?;
    writeln!(file, "Architecture: {}", std::env::consts::ARCH)?;
    writeln!(file, "Process: {}", std::process::id())?;
    writeln!(
        file,
        "Thread: {} ({:?})",
        thread.name().unwrap_or("<unnamed>"),
        thread.id()
    )?;
    writeln!(file, "Panic: {info}")?;
    writeln!(file, "\nBacktrace:\n{}", Backtrace::force_capture())?;
    file.flush()?;
    Ok(path)
}

fn filter(level: LoggingLevel) -> Result<EnvFilter> {
    let level = match level {
        LoggingLevel::Error => "error",
        LoggingLevel::Warn => "warn",
        LoggingLevel::Info => "info",
        LoggingLevel::Debug => "debug",
        LoggingLevel::Trace => "trace",
    };

    let directives = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| format!("off,upstream={level},upstream_rs={level}"));

    EnvFilter::builder()
        .with_regex(false)
        .parse(directives)
        .context("Invalid RUST_LOG filter")
}

struct LogWriter {
    file: Mutex<File>,
    path: PathBuf,
    config: LoggingConfig,
}

impl LogWriter {
    fn new(path: &Path, config: LoggingConfig) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create(true).append(true).open(path)?;
        retain_records(path, config)?;
        Ok(Self {
            file: Mutex::new(file),
            path: path.to_owned(),
            config,
        })
    }
}

struct LogGuard<'a> {
    file: MutexGuard<'a, File>,
    writer: &'a LogWriter,
}

impl<'a> MakeWriter<'a> for LogWriter {
    type Writer = LogGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        LogGuard {
            file: self.file.lock().unwrap_or_else(|error| error.into_inner()),
            writer: self,
        }
    }
}

impl Write for LogGuard<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.file.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl Drop for LogGuard<'_> {
    fn drop(&mut self) {
        let max = self.writer.config.max_size_bytes();
        if max > 0
            && self
                .file
                .metadata()
                .is_ok_and(|metadata| metadata.len() > max)
        {
            // The guard covers the entire formatted event, so retention never
            // sees a partially written JSON line from another thread.
            let _ = retain_records(&self.writer.path, self.writer.config);
        }
    }
}

fn retain_records(path: &Path, config: LoggingConfig) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let mut bytes = 0_u64;
    let mut start = content.len();
    let max = config.max_size_bytes();
    for (count, line) in content.split_inclusive('\n').rev().enumerate() {
        let next_bytes = bytes.saturating_add(line.len() as u64);
        if count >= config.vacuum || (max > 0 && next_bytes > max) {
            break;
        }

        bytes = next_bytes;
        start -= line.len();
    }

    if start > 0 {
        // Windows append-only handles cannot truncate. Rewrite through a
        // separate write handle, preserving the logger's append semantics.
        fs::write(path, &content.as_bytes()[start..])?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::temp_root;

    // Run panics in a separate process to avoid changing other tests' global hook.
    #[test]
    #[ignore = "subprocess helper for panic hook tests"]
    fn panic_hook_child() {
        let Some(directory) = std::env::var_os("UPSTREAM_PANIC_TEST_DIR") else {
            return;
        };

        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = writeln!(io::stderr().lock(), "Previous panic hook called");
            previous(info);
        }));
        init(
            &PathBuf::from(directory).join(LOG_FILE),
            LoggingConfig {
                enabled: false,
                ..LoggingConfig::default()
            },
        )
        .unwrap();
        let _ = std::thread::Builder::new()
            .name("panic-report-worker".to_owned())
            .spawn(|| panic!("panic report test"))
            .unwrap()
            .join();

        std::process::exit(101);
    }

    fn run_panic_child(directory: &Path) -> String {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "logging::tests::panic_hook_child",
                "--ignored",
                "--nocapture",
            ])
            .env("UPSTREAM_PANIC_TEST_DIR", directory)
            .env("RUST_BACKTRACE", "0")
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(101));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("Previous panic hook called"));
        assert!(stderr.contains("panic report test"));
        assert!(stderr.contains("panicked at"));
        stderr
    }

    #[test]
    fn panic_hook_writes_report_even_when_tracing_is_disabled() {
        let directory = temp_root("upstream-logging", "panic");
        let stderr = run_panic_child(&directory);
        let files: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();

        assert_eq!(files.len(), 1);
        let path = &files[0];
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("crash-")
        );

        assert_eq!(path.extension().unwrap(), "txt");
        assert!(stderr.contains(&path.display().to_string()));
        let report = fs::read_to_string(path).unwrap();
        for expected in [
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            "panic-report-worker",
            "panic report test",
            "logging.rs",
            "Backtrace:",
        ] {
            assert!(report.contains(expected), "Report missing {expected}");
        }

        assert!(!report.contains("disabled backtrace"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn panic_hook_preserves_normal_output_when_report_cannot_be_written() {
        let path = temp_root("upstream-logging", "panic-invalid-directory");
        fs::write(&path, "not a directory").unwrap();
        let stderr = run_panic_child(&path);
        assert!(!stderr.contains("Crash report written"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not a directory");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn source_urls_omit_credentials_queries_and_fragments() {
        assert_eq!(
            source_url(
                "https://user:private@example.com:8443/releases/tool.tar.gz?token=private#private"
            ),
            "https://example.com:8443/releases/tool.tar.gz"
        );

        assert_eq!(source_url("not a URL: private"), "<invalid URL>");
    }

    #[test]
    fn retention_keeps_newest_complete_records_within_both_limits() {
        let path = temp_root("upstream-logging", "retention");
        let line = format!("{}\n", "é".repeat(256 * 1024 - 1));
        fs::write(&path, format!("old\n{line}{line}{line}")).unwrap();
        let config = LoggingConfig {
            vacuum: 3,
            max_size_mb: 1,
            ..LoggingConfig::default()
        };

        let writer = LogWriter::new(&path, config).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), line.repeat(2));
        {
            let mut guard = writer.make_writer();
            guard.write_all(b"new\n").unwrap();
        }

        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{line}new\n"));
        drop(writer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn startup_retention_enforces_record_limit_without_size_limit() {
        let path = temp_root("upstream-logging", "count");
        fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let writer = LogWriter::new(
            &path,
            LoggingConfig {
                vacuum: 2,
                max_size_mb: 0,
                ..LoggingConfig::default()
            },
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "two\nthree\n");
        drop(writer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn logging_can_be_read_before_legacy_configuration_migrations() {
        let path = temp_root("upstream-logging", "legacy-config");
        fs::write(
            &path,
            "[trust]\nlegacy = true\n[logging]\nenabled = false\nlevel = 'trace'\n",
        )
        .unwrap();
        let config = read_config(&path).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.level, LoggingLevel::Trace);
        fs::remove_file(path).unwrap();
    }
}
