//! Configuration types for the backend.

use std::path::PathBuf;

/// Configuration for a single QMDB partition.
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Path to the partition directory.
    pub path: PathBuf,
    /// Maximum number of entries (for sizing).
    pub max_entries: usize,
}

impl PartitionConfig {
    /// Create a new partition config.
    pub const fn new(path: PathBuf, max_entries: usize) -> Self {
        Self { path, max_entries }
    }
}

/// Configuration for the full QMDB backend.
#[derive(Debug, Clone)]
pub struct QmdbBackendConfig {
    /// Configuration for the accounts partition.
    pub accounts: PartitionConfig,
    /// Configuration for the storage partition.
    pub storage: PartitionConfig,
    /// Configuration for the code partition.
    pub code: PartitionConfig,
}

impl QmdbBackendConfig {
    /// Create a new backend config with all partitions under a base path.
    pub fn new(base_path: PathBuf, max_entries: usize) -> Self {
        Self {
            accounts: PartitionConfig::new(base_path.join("accounts"), max_entries),
            storage: PartitionConfig::new(base_path.join("storage"), max_entries),
            code: PartitionConfig::new(base_path.join("code"), max_entries),
        }
    }

    /// Create config with custom partition sizes.
    pub fn with_sizes(
        base_path: PathBuf,
        accounts_max: usize,
        storage_max: usize,
        code_max: usize,
    ) -> Self {
        Self {
            accounts: PartitionConfig::new(base_path.join("accounts"), accounts_max),
            storage: PartitionConfig::new(base_path.join("storage"), storage_max),
            code: PartitionConfig::new(base_path.join("code"), code_max),
        }
    }
}
