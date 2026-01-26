//! Commonware-based QMDB backend implementation.

use alloy_primitives::B256;
use async_trait::async_trait;
use kora_handlers::{HandleError, RootProvider};
use kora_qmdb::StateRoot;

use crate::{AccountStore, BackendError, CodeStore, QmdbBackendConfig, StorageStore};

/// Commonware-based QMDB backend.
///
/// Provides storage for accounts, storage slots, and code using
/// commonware-storage primitives.
#[derive(Debug)]
pub struct CommonwareBackend {
    accounts: AccountStore,
    storage: StorageStore,
    code: CodeStore,
}

impl CommonwareBackend {
    /// Create a new backend with default in-memory stores.
    pub fn new() -> Self {
        Self { accounts: AccountStore::new(), storage: StorageStore::new(), code: CodeStore::new() }
    }

    /// Open a backend with the given configuration.
    ///
    /// Note: Currently this creates in-memory stores regardless of config.
    /// Full persistent storage will be added when commonware-storage QMDB
    /// is available.
    pub async fn open(_config: QmdbBackendConfig) -> Result<Self, BackendError> {
        // For now, we just create in-memory stores
        // In the future, this will use the config to open persistent stores
        Ok(Self::new())
    }

    /// Get a reference to the accounts store.
    pub const fn accounts(&self) -> &AccountStore {
        &self.accounts
    }

    /// Get a mutable reference to the accounts store.
    pub const fn accounts_mut(&mut self) -> &mut AccountStore {
        &mut self.accounts
    }

    /// Get a reference to the storage store.
    pub const fn storage(&self) -> &StorageStore {
        &self.storage
    }

    /// Get a mutable reference to the storage store.
    pub const fn storage_mut(&mut self) -> &mut StorageStore {
        &mut self.storage
    }

    /// Get a reference to the code store.
    pub const fn code(&self) -> &CodeStore {
        &self.code
    }

    /// Get a mutable reference to the code store.
    pub const fn code_mut(&mut self) -> &mut CodeStore {
        &mut self.code
    }

    /// Get the current state root.
    pub async fn get_state_root(&self) -> Result<B256, BackendError> {
        let accounts_root = self.accounts.root().await?;
        let storage_root = self.storage.root().await?;
        let code_root = self.code.root().await?;
        Ok(StateRoot::compute(accounts_root, storage_root, code_root))
    }

    /// Compute the state root.
    ///
    /// This is the same as `get_state_root` for now since we compute
    /// roots incrementally.
    pub async fn compute_state_root(&mut self) -> Result<B256, BackendError> {
        self.get_state_root().await
    }

    /// Commit pending changes and return the new state root.
    ///
    /// For now, this just computes the root since we apply changes
    /// immediately in write_batch.
    pub async fn commit(&mut self) -> Result<B256, BackendError> {
        self.get_state_root().await
    }
}

impl Default for CommonwareBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RootProvider for CommonwareBackend {
    async fn state_root(&self) -> Result<B256, HandleError> {
        self.get_state_root().await.map_err(|e| HandleError::RootComputation(e.to_string()))
    }

    async fn compute_root(&mut self) -> Result<B256, HandleError> {
        self.compute_state_root().await.map_err(|e| HandleError::RootComputation(e.to_string()))
    }

    async fn commit_and_get_root(&mut self) -> Result<B256, HandleError> {
        self.commit().await.map_err(|e| HandleError::RootComputation(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn backend_new() {
        let backend = CommonwareBackend::new();
        let root = backend.get_state_root().await.unwrap();
        // Initial root should be deterministic (based on empty stores)
        assert_ne!(root, B256::ZERO); // MMR has non-zero initial root
    }

    #[tokio::test]
    async fn backend_default() {
        let backend = CommonwareBackend::default();
        assert!(backend.accounts().root().await.is_ok());
    }

    #[tokio::test]
    async fn backend_open() {
        let config = QmdbBackendConfig::new(PathBuf::from("/tmp/test"), 1000);
        let backend = CommonwareBackend::open(config).await.unwrap();
        assert!(backend.get_state_root().await.is_ok());
    }
}
