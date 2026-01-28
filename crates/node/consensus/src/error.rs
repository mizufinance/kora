//! Error types for consensus operations.

use kora_domain::{ConsensusDigest, StateRoot};
use thiserror::Error;

/// Error type for consensus operations.
#[derive(Debug, Error)]
pub enum ConsensusError {
    /// Parent block not found.
    #[error("parent not found: {0:?}")]
    ParentNotFound(ConsensusDigest),

    /// Snapshot not found for digest.
    #[error("snapshot not found: {0:?}")]
    SnapshotNotFound(ConsensusDigest),

    /// Execution failed.
    #[error("execution failed: {0}")]
    Execution(String),

    /// State database error.
    #[error("state db error: {0}")]
    StateDb(#[from] kora_traits::StateDbError),

    /// Block validation failed.
    #[error("validation failed: {0}")]
    Validation(String),

    /// State root mismatch.
    #[error("state root mismatch: expected {expected:?}, got {actual:?}")]
    StateRootMismatch {
        /// Expected state root.
        expected: StateRoot,
        /// Actual state root.
        actual: StateRoot,
    },
}
