// file: src/error.rs
// description: Error types for ARTIFACT

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ArtifactError>;

#[derive(Debug, Error)]
pub enum ArtifactError {
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

impl ArtifactError {
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
