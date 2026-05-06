//! Tracing-based logging initialization with optional file rotation.

use std::path::PathBuf;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// Parameters controlling how tracing output is routed and formatted.
pub struct LoggingConfig {
    /// Directory where rolling log files are written (when `log_to_file` is set).
    pub log_dir: PathBuf,
    /// Minimum level filter string (e.g. `"info"`, `"debug"`).
    pub log_level: String,
    /// Whether to write log events to a rolling daily file.
    pub log_to_file: bool,
    /// Whether to write log events to standard output.
    pub log_to_stdout: bool,
    /// Whether to format log lines as JSON objects.
    pub json_format: bool,
}

/// Holds the [`tracing_appender`] worker guard for the lifetime of the process.
///
/// Dropping this guard flushes and closes any in-flight file-appender buffers.
/// Keep it alive by storing it in `main` until the process exits.
pub struct LoggingGuard {
    _guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// Initialize the global tracing subscriber from the given [`LoggingConfig`].
///
/// Returns a [`LoggingGuard`] whose drop flushes the non-blocking file writer.
/// Returns an error if a global subscriber has already been set.
pub fn init_logging(config: LoggingConfig) -> anyhow::Result<LoggingGuard> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

    if config.log_to_file {
        std::fs::create_dir_all(&config.log_dir)?;

        let file_appender = tracing_appender::rolling::daily(&config.log_dir, "artifact.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        if config.log_to_stdout {
            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_writer(std::io::stdout))
                .with(fmt::layer().with_writer(non_blocking));
            tracing::subscriber::set_global_default(subscriber)?;
        } else {
            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_writer(non_blocking));
            tracing::subscriber::set_global_default(subscriber)?;
        }

        Ok(LoggingGuard {
            _guard: Some(guard),
        })
    } else if config.log_to_stdout {
        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_writer(std::io::stdout));
        tracing::subscriber::set_global_default(subscriber)?;

        Ok(LoggingGuard { _guard: None })
    } else {
        Ok(LoggingGuard { _guard: None })
    }
}
