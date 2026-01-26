//! Storage store backed by commonware-storage.

use alloy_primitives::{B256, U256, keccak256};
use kora_qmdb::{QmdbBatchable, QmdbGettable, StorageKey};
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::error::BackendError;

/// Storage store backed by commonware-storage.
///
/// Uses an in-memory hashmap with keccak256-based root computation.
/// This is a placeholder implementation that will be replaced with
/// full commonware QMDB integration when available.
#[derive(Debug)]
pub struct StorageStore {
    /// In-memory storage for storage slots.
    data: RwLock<HashMap<StorageKey, U256>>,
    /// Cached root hash.
    root_cache: RwLock<B256>,
}

impl StorageStore {
    /// Create a new storage store.
    pub fn new() -> Self {
        Self { data: RwLock::new(HashMap::new()), root_cache: RwLock::new(B256::ZERO) }
    }

    /// Get the root hash of the storage store.
    pub async fn root(&self) -> Result<B256, BackendError> {
        Ok(*self.root_cache.read().await)
    }

    /// Compute root from current data.
    async fn compute_root(&self) -> B256 {
        let data = self.data.read().await;
        if data.is_empty() {
            return B256::ZERO;
        }
        // Simple root computation: hash all sorted entries
        let mut entries: Vec<_> = data.iter().collect();
        entries.sort_by_key(|(k, _)| k.to_bytes());

        let mut hasher_input = Vec::new();
        for (key, value) in entries {
            hasher_input.extend_from_slice(&key.to_bytes());
            hasher_input.extend_from_slice(&value.to_be_bytes::<32>());
        }
        keccak256(&hasher_input)
    }
}

impl Default for StorageStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for storage store operations.
#[derive(Debug)]
pub struct StorageStoreError(pub String);

impl std::fmt::Display for StorageStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "storage store error: {}", self.0)
    }
}

impl std::error::Error for StorageStoreError {}

impl QmdbGettable for StorageStore {
    type Key = StorageKey;
    type Value = U256;
    type Error = StorageStoreError;

    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Value>, Self::Error> {
        let data = self.data.read().await;
        Ok(data.get(key).copied())
    }
}

impl QmdbBatchable for StorageStore {
    async fn write_batch<I>(&mut self, ops: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = (Self::Key, Option<Self::Value>)> + Send,
        I::IntoIter: Send,
    {
        let mut data = self.data.write().await;

        for (key, value) in ops {
            match value {
                Some(v) => {
                    data.insert(key, v);
                }
                None => {
                    data.remove(&key);
                }
            }
        }
        drop(data);

        // Update root cache
        let new_root = self.compute_root().await;
        *self.root_cache.write().await = new_root;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;

    #[tokio::test]
    async fn storage_store_get_missing() {
        let store = StorageStore::new();
        let key = StorageKey::new(Address::ZERO, 0, U256::from(1));
        let result = store.get(&key).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn storage_store_write_and_get() {
        let mut store = StorageStore::new();
        let key = StorageKey::new(Address::repeat_byte(0x01), 0, U256::from(1));
        let value = U256::from(42);

        store.write_batch(vec![(key, Some(value))]).await.unwrap();

        let result = store.get(&key).await.unwrap();
        assert_eq!(result, Some(value));
    }

    #[tokio::test]
    async fn storage_store_delete() {
        let mut store = StorageStore::new();
        let key = StorageKey::new(Address::repeat_byte(0x01), 0, U256::from(1));
        let value = U256::from(42);

        store.write_batch(vec![(key, Some(value))]).await.unwrap();
        assert!(store.get(&key).await.unwrap().is_some());

        store.write_batch(vec![(key, None)]).await.unwrap();
        assert!(store.get(&key).await.unwrap().is_none());
    }
}
