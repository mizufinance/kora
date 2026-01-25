//! Partition state machine for managing QMDB database lifecycle.

use crate::error::BackendError;

/// State of a partition database.
#[derive(Debug)]
pub enum PartitionState<D> {
    /// Database not yet initialized.
    Uninitialized,
    /// Database is ready for operations.
    Ready(D),
    /// Database has been closed.
    Closed,
}

impl<D> PartitionState<D> {
    /// Create a new uninitialized partition state.
    pub const fn new() -> Self {
        Self::Uninitialized
    }

    /// Initialize the partition with a database.
    pub fn initialize(&mut self, db: D) -> Result<(), BackendError> {
        match self {
            Self::Uninitialized => {
                *self = Self::Ready(db);
                Ok(())
            }
            Self::Ready(_) => Err(BackendError::Partition("already initialized".to_string())),
            Self::Closed => Err(BackendError::Partition("partition closed".to_string())),
        }
    }

    /// Get a reference to the database if ready.
    pub fn get(&self) -> Result<&D, BackendError> {
        match self {
            Self::Ready(db) => Ok(db),
            Self::Uninitialized => Err(BackendError::NotInitialized),
            Self::Closed => Err(BackendError::Partition("partition closed".to_string())),
        }
    }

    /// Get a mutable reference to the database if ready.
    pub fn get_mut(&mut self) -> Result<&mut D, BackendError> {
        match self {
            Self::Ready(db) => Ok(db),
            Self::Uninitialized => Err(BackendError::NotInitialized),
            Self::Closed => Err(BackendError::Partition("partition closed".to_string())),
        }
    }

    /// Close the partition and return the database.
    pub fn close(&mut self) -> Result<D, BackendError> {
        match std::mem::replace(self, Self::Closed) {
            Self::Ready(db) => Ok(db),
            Self::Uninitialized => Err(BackendError::NotInitialized),
            Self::Closed => Err(BackendError::Partition("already closed".to_string())),
        }
    }

    /// Check if the partition is ready.
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }
}

impl<D> Default for PartitionState<D> {
    fn default() -> Self {
        Self::new()
    }
}
