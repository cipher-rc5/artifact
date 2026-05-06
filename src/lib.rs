//! Library interface for the ARTIFACT disk-space analyzer.
//!
//! Provides the scanner, database, configuration, and utility types used by
//! the GPUI desktop application. The binary in `src/main.rs` is the primary
//! consumer; this lib target enables integration testing against real
//! filesystem and database operations.

pub mod components;
pub mod config;
pub mod database;
pub mod directory_item;
pub mod error;
pub mod logging;
pub mod rules;
pub mod scanner;
pub mod theme;
pub mod utils;

// Re-exports for convenience
pub use config::{AppConfig, DeleteMode};
pub use database::{DeletionDatabase, DeletionRecord, DeletionStatistics};
pub use directory_item::{DirectoryItem, DirectoryType};
pub use error::{ArtifactError, Result};
pub use logging::{LoggingConfig, LoggingGuard};
pub use scanner::Scanner;
pub use theme::{BentoTheme, DesignSystem};
