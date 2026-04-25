// file: src/error.rs
// description: Error types for Space Cleaner

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SpaceCleanerError>;

#[derive(Debug, Error)]
pub enum SpaceCleanerError {
    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Database initialization error: {0}")]
    DatabaseInit(String),

    #[error("Database connection error: {0}")]
    DatabaseConnection(String),

    #[error("Database query error: {0}")]
    DatabaseQuery(String),

    #[error("Scan error: {0}")]
    Scan(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path error: {0}")]
    Path(String),
}

impl SpaceCleanerError {
    pub fn user_message(&self) -> String {
        match self {
            Self::Configuration(msg) => format!("Configuration problem: {}", msg),
            Self::DatabaseInit(msg) => format!("Could not initialize database: {}", msg),
            Self::DatabaseConnection(msg) => format!("Database connection failed: {}", msg),
            Self::DatabaseQuery(msg) => format!("Database query failed: {}", msg),
            Self::Scan(msg) => format!("Scan failed: {}", msg),
            Self::Io(e) => format!("IO error: {}", e),
            Self::Path(msg) => format!("Path error: {}", msg),
        }
    }
}

impl From<redb::Error> for SpaceCleanerError {
    fn from(e: redb::Error) -> Self {
        SpaceCleanerError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::DatabaseError> for SpaceCleanerError {
    fn from(e: redb::DatabaseError) -> Self {
        SpaceCleanerError::DatabaseConnection(e.to_string())
    }
}

impl From<redb::TransactionError> for SpaceCleanerError {
    fn from(e: redb::TransactionError) -> Self {
        SpaceCleanerError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::TableError> for SpaceCleanerError {
    fn from(e: redb::TableError) -> Self {
        SpaceCleanerError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::StorageError> for SpaceCleanerError {
    fn from(e: redb::StorageError) -> Self {
        SpaceCleanerError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::CommitError> for SpaceCleanerError {
    fn from(e: redb::CommitError) -> Self {
        SpaceCleanerError::DatabaseQuery(e.to_string())
    }
}
