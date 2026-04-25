// file: src/lib.rs
// description: Public library interface for Space Cleaner (used by tests)
// reference: https://github.com/zed-industries/zed

pub mod components;
pub mod config;
pub mod database;
pub mod directory_item;
pub mod error;
pub mod logging;
pub mod scanner;
pub mod theme;
pub mod utils;

// Re-exports for convenience
pub use config::AppConfig;
pub use database::{DeletionDatabase, DeletionRecord, DeletionStatistics};
pub use directory_item::{DirectoryItem, DirectoryType};
pub use error::{Result, SpaceCleanerError};
pub use logging::{LoggingConfig, LoggingGuard};
pub use scanner::Scanner;
pub use theme::{BentoTheme, DesignSystem};
