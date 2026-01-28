//! Configuration types for the backend.

use commonware_runtime::buffer::PoolRef;

/// Configuration for the full QMDB backend.
#[derive(Clone)]
pub struct QmdbBackendConfig {
    /// Prefix used to derive partition names.
    pub partition_prefix: String,
    /// Buffer pool shared by underlying QMDB partitions.
    pub buffer_pool: PoolRef,
}

impl QmdbBackendConfig {
    /// Create a new backend config for the given partition prefix.
    pub fn new(partition_prefix: impl Into<String>, buffer_pool: PoolRef) -> Self {
        Self { partition_prefix: partition_prefix.into(), buffer_pool }
    }
}

impl std::fmt::Debug for QmdbBackendConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QmdbBackendConfig")
            .field("partition_prefix", &self.partition_prefix)
            .finish()
    }
}
