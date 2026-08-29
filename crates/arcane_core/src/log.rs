//! Logging façade. Wraps `log` crate with an `env_logger`-style initialization
//! and a stable `LogLevel` enum used elsewhere in the engine.

use log::{Level, LevelFilter};
use once_cell::sync::OnceCell;

/// Mystical Arcana log level — mirrors `log::Level` but is a stable enum
/// (no transitive dependency leakage for downstream crates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum LogLevel {
    /// Errors are fatal or near-fatal; the simulation may be in a bad state.
    Error = 0,
    ///Warnings — recoverable but worth flagging (asset missing, fallback used).
    Warn = 1,
    /// Informational — milestones during normal operation.
    Info = 2,
    /// Debug — useful during development.
    Debug = 3,
    /// Trace — very high-volume, only when chasing a specific subsystem.
    Trace = 4,
}

impl From<LogLevel> for Level {
    fn from(l: LogLevel) -> Self {
        match l {
            LogLevel::Error => Level::Error,
            LogLevel::Warn => Level::Warn,
            LogLevel::Info => Level::Info,
            LogLevel::Debug => Level::Debug,
            LogLevel::Trace => Level::Trace,
        }
    }
}

impl From<LogLevel> for LevelFilter {
    fn from(l: LogLevel) -> Self {
        match l {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
        }
    }
}

static INITIALIZED: OnceCell<()> = OnceCell::new();

/// Initializes the global logger with the given maximum level. Idempotent —
/// calling more than once is a no-op. Safe to call from any thread.
pub fn init_logger(level: LogLevel) {
    INITIALIZED.get_or_init(|| {
        let _ = env_logger::Builder::new()
            .filter_level(level.into())
            .format_timestamp_millis()
            .try_init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Error < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Trace);
    }
    #[test]
    fn init_logger_idempotent() {
        // Calling twice should not panic.
        init_logger(LogLevel::Info);
        init_logger(LogLevel::Info);
    }
}
