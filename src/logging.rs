// file: src/logging.rs
// description: Logging configuration with tracing

use std::path::PathBuf;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

pub struct LoggingConfig {
    pub log_dir: PathBuf,
    pub log_level: String,
    pub log_to_file: bool,
    pub log_to_stdout: bool,
    pub json_format: bool,
}

pub struct LoggingGuard {
    _guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

pub fn init_logging(config: LoggingConfig) -> anyhow::Result<LoggingGuard> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

    if config.log_to_file {
        std::fs::create_dir_all(&config.log_dir)?;

        let file_appender = tracing_appender::rolling::daily(&config.log_dir, "space_cleaner.log");
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
