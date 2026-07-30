//! Central, structured logging (§41 step 2: "Initialize Logging").
//!
//! "No module creates its own logger" -- every crate routes through the
//! single central `Logger` obtained via [`logger()`], using the
//! [`log_info!`], [`log_warn!`], [`log_error!`], [`log_debug!`], and
//! [`log_trace!`] macros (or [`Logger::log`] directly). The concrete sink
//! (console, file, both) is configured once at startup (§41) via
//! [`init`]; before `init` is called, the logger falls back to a
//! console-only sink so early startup messages are never lost.

use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

/// Log severity, ordered from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        };
        write!(f, "{label}")
    }
}

/// Where log lines are written. Selected once at startup (§41); never
/// hardcoded into call sites.
enum Sink {
    Console,
    File(Mutex<std::fs::File>),
    Both(Mutex<std::fs::File>),
}

/// The central logger. A single instance lives behind [`logger()`].
pub struct Logger {
    sink: Sink,
    min_level: LogLevel,
}

impl Logger {
    fn console_only() -> Self {
        Self {
            sink: Sink::Console,
            min_level: LogLevel::Info,
        }
    }

    /// Format one log line: `[LEVEL] target: message`.
    fn format(level: LogLevel, target: &str, message: &str) -> String {
        format!("[{level}] {target}: {message}")
    }

    /// Log a message at the given level from the given target (typically
    /// `module_path!()`). No-op if `level` is more verbose than the
    /// configured minimum.
    pub fn log(&self, level: LogLevel, target: &str, message: &str) {
        if level > self.min_level {
            return;
        }
        let line = Self::format(level, target, message);
        match &self.sink {
            Sink::Console => {
                println!("{line}");
            }
            Sink::File(file) => {
                if let Ok(mut f) = file.lock() {
                    let _ = writeln!(f, "{line}");
                }
            }
            Sink::Both(file) => {
                println!("{line}");
                if let Ok(mut f) = file.lock() {
                    let _ = writeln!(f, "{line}");
                }
            }
        }
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Where the central logger should write (§23: storage locations are
/// configuration, never hardcoded into this module -- the caller resolves
/// the actual path from `SettingsProvider` before calling `init`).
pub enum LogDestination<'a> {
    ConsoleOnly,
    FileOnly { path: &'a str },
    ConsoleAndFile { path: &'a str },
}

/// Initialize the central logger (§41 step 2). Safe to call at most once;
/// subsequent calls are no-ops so a crate cannot silently steal the sink
/// configured by app-tauri's startup sequence.
pub fn init(destination: LogDestination<'_>, min_level: LogLevel) {
    let sink = match destination {
        LogDestination::ConsoleOnly => Sink::Console,
        LogDestination::FileOnly { path } => match open_log_file(path) {
            Ok(file) => Sink::File(Mutex::new(file)),
            Err(_) => Sink::Console,
        },
        LogDestination::ConsoleAndFile { path } => match open_log_file(path) {
            Ok(file) => Sink::Both(Mutex::new(file)),
            Err(_) => Sink::Console,
        },
    };
    let _ = LOGGER.set(Logger { sink, min_level });
}

fn open_log_file(path: &str) -> std::io::Result<std::fs::File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// The central logger (§41). Falls back to a console-only, `Info`-level
/// logger if [`init`] has not been called yet.
pub fn logger() -> &'static Logger {
    LOGGER.get_or_init(Logger::console_only)
}

/// Placeholder for the logging initialization step in the startup sequence
/// (§41). Kept for backward compatibility with earlier call sites; prefer
/// calling [`init`] directly with a real destination.
pub fn init_logging() {
    let _ = LOGGER.get_or_init(Logger::console_only);
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Error, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Warn, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Info, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Debug, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Trace, module_path!(), &format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_ordering_is_error_most_severe() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }

    #[test]
    fn format_includes_level_target_and_message() {
        let line = Logger::format(LogLevel::Warn, "atlas_utils::logging::tests", "hello");
        assert_eq!(line, "[WARN] atlas_utils::logging::tests: hello");
    }

    #[test]
    fn console_logger_does_not_panic_below_min_level() {
        let logger = Logger::console_only();
        // Trace is more verbose than the default Info minimum -- must be a
        // silent no-op, not a panic or a write.
        logger.log(LogLevel::Trace, "test", "should be suppressed");
    }

    #[test]
    fn file_sink_writes_expected_line() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("atlas-utils-test-{}.log", std::process::id()));
        let path_str = path.to_str().unwrap();

        let logger = Logger {
            sink: Sink::File(Mutex::new(open_log_file(path_str).unwrap())),
            min_level: LogLevel::Debug,
        };
        logger.log(LogLevel::Info, "test::target", "written to file");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[INFO] test::target: written to file"));
        let _ = std::fs::remove_file(&path);
    }
}
