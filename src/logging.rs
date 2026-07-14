//! Tracing-based logging initialization with optional file rotation.

use std::path::PathBuf;
use tracing_subscriber::EnvFilter;
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

/// Build the [`EnvFilter`] used by the subscriber.
///
/// **Precedence:** the `RUST_LOG` environment variable, when present and valid,
/// takes priority over `config_level` and silently overrides the configured
/// level for the whole process. This mirrors the conventional behavior of
/// `tracing_subscriber` and lets an operator crank up verbosity at launch
/// (`RUST_LOG=debug ...`) without editing the config file. When `RUST_LOG` is
/// unset or unparseable, `config_level` (e.g. from `config.toml`) is used.
fn build_env_filter(config_level: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(config_level))
}

/// Initialize the global tracing subscriber from the given [`LoggingConfig`].
///
/// Returns a [`LoggingGuard`] whose drop flushes the non-blocking file writer.
/// Returns an error if a global subscriber has already been set.
///
/// # Log level precedence
///
/// The effective level filter is chosen by [`build_env_filter`]: the `RUST_LOG`
/// environment variable, if set and valid, **silently overrides**
/// [`LoggingConfig::log_level`] (and hence the configured `log_level` in
/// `config.toml`) for the entire process. This is intentional — it allows
/// ad-hoc debugging via `RUST_LOG=debug` without touching the config — but it
/// does mean the configured level is not authoritative when `RUST_LOG` is
/// present.
///
/// # Output format
///
/// When [`LoggingConfig::json_format`] is `true`, both the file and stdout
/// layers emit newline-delimited JSON objects instead of the default
/// human-readable format.
pub fn init_logging(config: LoggingConfig) -> anyhow::Result<LoggingGuard> {
    let filter = build_env_filter(&config.log_level);
    let json = config.json_format;

    if config.log_to_file {
        std::fs::create_dir_all(&config.log_dir)?;

        let file_appender = tracing_appender::rolling::daily(&config.log_dir, "artifact.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        if config.log_to_stdout {
            // File + stdout.
            if json {
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().json().with_writer(std::io::stdout))
                    .with(fmt::layer().json().with_writer(non_blocking));
                tracing::subscriber::set_global_default(subscriber)?;
            } else {
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().with_writer(std::io::stdout))
                    .with(fmt::layer().with_writer(non_blocking));
                tracing::subscriber::set_global_default(subscriber)?;
            }
        } else {
            // File only.
            if json {
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().json().with_writer(non_blocking));
                tracing::subscriber::set_global_default(subscriber)?;
            } else {
                let subscriber = tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt::layer().with_writer(non_blocking));
                tracing::subscriber::set_global_default(subscriber)?;
            }
        }

        Ok(LoggingGuard {
            _guard: Some(guard),
        })
    } else if config.log_to_stdout {
        // Stdout only.
        if json {
            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json().with_writer(std::io::stdout));
            tracing::subscriber::set_global_default(subscriber)?;
        } else {
            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_writer(std::io::stdout));
            tracing::subscriber::set_global_default(subscriber)?;
        }

        Ok(LoggingGuard { _guard: None })
    } else {
        Ok(LoggingGuard { _guard: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    // NOTE: `init_logging` installs a *global* subscriber, which can only be set
    // once per process. We therefore do not call it here; instead we test the
    // pure helpers that decide filter/format so the logic is covered without
    // fighting global state.

    // These tests mutate the shared process-wide `RUST_LOG` env var, so they must
    // not run concurrently with one another.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn env_filter_uses_config_level_when_rust_log_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: ENV_LOCK serializes all RUST_LOG access across these tests.
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
        let filter = build_env_filter("warn");
        // EnvFilter's Display is its directive string.
        assert_eq!(filter.to_string(), "warn");
    }

    #[test]
    fn env_filter_accepts_various_levels() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: ENV_LOCK serializes all RUST_LOG access across these tests.
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
        for level in ["error", "warn", "info", "debug", "trace"] {
            let filter = build_env_filter(level);
            assert_eq!(filter.to_string(), level);
        }
    }

    #[test]
    fn env_filter_rust_log_overrides_config_level() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Document/verify the documented precedence: RUST_LOG wins.
        // SAFETY: ENV_LOCK serializes all RUST_LOG access across these tests.
        unsafe {
            std::env::set_var("RUST_LOG", "debug");
        }
        let filter = build_env_filter("error");
        assert_eq!(filter.to_string(), "debug");
        unsafe {
            std::env::remove_var("RUST_LOG");
        }
    }
}
