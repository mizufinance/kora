//! Error types for the backend.

use thiserror::Error;

/// Error type for backend operations.
#[derive(Debug, Error)]
pub enum BackendError {
    /// Storage I/O error.
    #[error("storage error: {0}")]
    Storage(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// Database not initialized.
    #[error("database not initialized")]
    NotInitialized,

    /// Partition error.
    #[error("partition error: {0}")]
    Partition(String),

    /// State root computation failed.
    #[error("root computation failed: {0}")]
    RootComputation(String),
}
