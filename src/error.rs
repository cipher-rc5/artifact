//! Crate-wide error types for ARTIFACT.

use thiserror::Error;

/// Convenience alias for `Result<T, ArtifactError>`.
pub type Result<T> = std::result::Result<T, ArtifactError>;

/// Application-level errors.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ArtifactError {
    /// A problem reading or applying the TOML configuration file.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// The redb database could not be created or opened.
    #[error("Database initialization error: {0}")]
    DatabaseInit(String),

    /// A connection-level redb error (e.g. version mismatch, corrupt file).
    #[error("Database connection error: {0}")]
    DatabaseConnection(String),

    /// A query, transaction, or commit failed at runtime.
    #[error("Database query error: {0}")]
    DatabaseQuery(String),

    /// The filesystem scan encountered an unrecoverable error.
    #[error("Scan error: {0}")]
    Scan(String),

    /// An underlying OS I/O error (file not found, permission denied, etc.).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A path could not be parsed or converted to a valid UTF-8 string.
    #[error("Path error: {0}")]
    Path(String),
}

impl ArtifactError {
    /// Return a human-readable message suitable for display in the UI.
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

impl From<redb::Error> for ArtifactError {
    fn from(e: redb::Error) -> Self {
        ArtifactError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::DatabaseError> for ArtifactError {
    fn from(e: redb::DatabaseError) -> Self {
        ArtifactError::DatabaseConnection(e.to_string())
    }
}

impl From<redb::TransactionError> for ArtifactError {
    fn from(e: redb::TransactionError) -> Self {
        ArtifactError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::TableError> for ArtifactError {
    fn from(e: redb::TableError) -> Self {
        ArtifactError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::StorageError> for ArtifactError {
    fn from(e: redb::StorageError) -> Self {
        ArtifactError::DatabaseQuery(e.to_string())
    }
}

impl From<redb::CommitError> for ArtifactError {
    fn from(e: redb::CommitError) -> Self {
        ArtifactError::DatabaseQuery(e.to_string())
    }
}
