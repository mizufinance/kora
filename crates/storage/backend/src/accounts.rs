//! Account store backed by commonware-storage.

use alloy_primitives::{Address, B256, keccak256};
use kora_qmdb::{AccountEncoding, QmdbBatchable, QmdbGettable};
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::error::BackendError;

/// Account store backed by commonware-storage.
///
/// Uses an in-memory hashmap with keccak256-based root computation.
/// This is a placeholder implementation that will be replaced with
/// full commonware QMDB integration when available.
#[derive(Debug)]
pub struct AccountStore {
    /// In-memory storage for accounts.
    data: RwLock<HashMap<Address, [u8; AccountEncoding::SIZE]>>,
    /// Cached root hash.
    root_cache: RwLock<B256>,
}

impl AccountStore {
    /// Create a new account store.
    pub fn new() -> Self {
        Self { data: RwLock::new(HashMap::new()), root_cache: RwLock::new(B256::ZERO) }
    }

    /// Get the root hash of the account store.
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
        for (addr, value) in entries {
            hasher_input.extend_from_slice(addr.as_slice());
            hasher_input.extend_from_slice(value);
        }
        keccak256(&hasher_input)
    }
}

impl Default for AccountStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for account store operations.
#[derive(Debug)]
pub struct AccountStoreError(pub String);

impl std::fmt::Display for AccountStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "account store error: {}", self.0)
    }
}

impl std::error::Error for AccountStoreError {}

impl QmdbGettable for AccountStore {
    type Key = Address;
    type Value = [u8; AccountEncoding::SIZE];
    type Error = AccountStoreError;

    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Value>, Self::Error> {
        let data = self.data.read().await;
        Ok(data.get(key).copied())
    }
}

impl QmdbBatchable for AccountStore {
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
    use alloy_primitives::U256;

    #[tokio::test]
    async fn account_store_get_missing() {
        let store = AccountStore::new();
        let result = store.get(&Address::ZERO).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn account_store_write_and_get() {
        let mut store = AccountStore::new();
        let addr = Address::repeat_byte(0x01);
        let encoded = AccountEncoding::encode(1, U256::from(1000), B256::ZERO, 0);

        store.write_batch(vec![(addr, Some(encoded))]).await.unwrap();

        let result = store.get(&addr).await.unwrap();
        assert!(result.is_some());
        let (nonce, balance, _, _) = AccountEncoding::decode(&result.unwrap()).unwrap();
        assert_eq!(nonce, 1);
        assert_eq!(balance, U256::from(1000));
    }

    #[tokio::test]
    async fn account_store_delete() {
        let mut store = AccountStore::new();
        let addr = Address::repeat_byte(0x01);
        let encoded = AccountEncoding::encode(1, U256::from(1000), B256::ZERO, 0);

        store.write_batch(vec![(addr, Some(encoded))]).await.unwrap();
        assert!(store.get(&addr).await.unwrap().is_some());

        store.write_batch(vec![(addr, None)]).await.unwrap();
        assert!(store.get(&addr).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn account_store_root_changes() {
        let mut store = AccountStore::new();
        let initial_root = store.root().await.unwrap();
        assert_eq!(initial_root, B256::ZERO);

        let addr = Address::repeat_byte(0x01);
        let encoded = AccountEncoding::encode(1, U256::from(1000), B256::ZERO, 0);
        store.write_batch(vec![(addr, Some(encoded))]).await.unwrap();

        let new_root = store.root().await.unwrap();
        assert_ne!(new_root, B256::ZERO);
    }
}
