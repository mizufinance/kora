//! Code store backed by commonware-storage.

use alloy_primitives::{keccak256, B256};
use kora_qmdb::{QmdbBatchable, QmdbGettable};
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::error::BackendError;

/// Code store backed by commonware-storage.
///
/// Uses an in-memory hashmap with keccak256-based root computation.
/// This is a placeholder implementation that will be replaced with
/// full commonware QMDB integration when available.
#[derive(Debug)]
pub struct CodeStore {
    /// In-memory storage for code.
    data: RwLock<HashMap<B256, Vec<u8>>>,
    /// Cached root hash.
    root_cache: RwLock<B256>,
}

impl CodeStore {
    /// Create a new code store.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            root_cache: RwLock::new(B256::ZERO),
        }
    }

    /// Get the root hash of the code store.
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
        entries.sort_by_key(|(k, _)| **k);

        let mut hasher_input = Vec::new();
        for (hash, code) in entries {
            hasher_input.extend_from_slice(hash.as_slice());
            hasher_input.extend_from_slice(code);
        }
        keccak256(&hasher_input)
    }
}

impl Default for CodeStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for code store operations.
#[derive(Debug)]
pub struct CodeStoreError(pub String);

impl std::fmt::Display for CodeStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "code store error: {}", self.0)
    }
}

impl std::error::Error for CodeStoreError {}

impl QmdbGettable for CodeStore {
    type Key = B256;
    type Value = Vec<u8>;
    type Error = CodeStoreError;

    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Value>, Self::Error> {
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
    }
}

impl QmdbBatchable for CodeStore {
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

    #[tokio::test]
    async fn code_store_get_missing() {
        let store = CodeStore::new();
        let result = store.get(&B256::ZERO).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn code_store_write_and_get() {
        let mut store = CodeStore::new();
        let hash = B256::repeat_byte(0x01);
        let code = vec![0x60, 0x00, 0x60, 0x00]; // Simple bytecode

        store.write_batch(vec![(hash, Some(code.clone()))]).await.unwrap();

        let result = store.get(&hash).await.unwrap();
        assert_eq!(result, Some(code));
    }

    #[tokio::test]
    async fn code_store_delete() {
        let mut store = CodeStore::new();
        let hash = B256::repeat_byte(0x01);
        let code = vec![0x60, 0x00];

        store.write_batch(vec![(hash, Some(code))]).await.unwrap();
        assert!(store.get(&hash).await.unwrap().is_some());

        store.write_batch(vec![(hash, None)]).await.unwrap();
        assert!(store.get(&hash).await.unwrap().is_none());
    }
}
