//! Error types for database handles.

use alloy_primitives::B256;
use kora_qmdb::QmdbError;
use thiserror::Error;

/// Error type for database handle operations.
#[derive(Debug, Error)]
pub enum HandleError {
    /// QMDB store error.
    #[error("qmdb error: {0}")]
    Qmdb(#[from] QmdbError),

    /// Lock was poisoned.
    #[error("lock poisoned")]
    LockPoisoned,

    /// Code not found for hash.
    #[error("code not found: {0}")]
    CodeNotFound(B256),

    /// Block hash not found.
    #[error("block hash not found: {0}")]
    BlockHashNotFound(u64),

    /// Root computation error.
    #[error("root computation error: {0}")]
    RootComputation(String),
}

impl revm::database_interface::DBErrorMarker for HandleError {}
