//! In-memory mempool implementation.

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use alloy_primitives::{B256, keccak256};

use crate::traits::{Mempool, TxId};

/// Simple in-memory mempool backed by a BTreeMap.
#[derive(Debug, Clone)]
pub struct InMemoryMempool {
    inner: Arc<RwLock<BTreeMap<TxId, Vec<u8>>>>,
}

impl InMemoryMempool {
    /// Create a new empty mempool.
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(BTreeMap::new())) }
    }
}

impl Default for InMemoryMempool {
    fn default() -> Self {
        Self::new()
    }
}

impl Mempool for InMemoryMempool {
    type Tx = Vec<u8>;

    fn insert(&self, tx: Self::Tx) -> bool {
        let id = keccak256(&tx);
        let mut inner = self.inner.write().unwrap();
        inner.insert(id, tx).is_none()
    }

    fn build(&self, max_txs: usize, excluded: &std::collections::BTreeSet<B256>) -> Vec<Self::Tx> {
        let inner = self.inner.read().unwrap();
        inner
            .iter()
            .filter(|(id, _)| !excluded.contains(*id))
            .take(max_txs)
            .map(|(_, tx)| tx.clone())
            .collect()
    }

    fn prune(&self, tx_ids: &[TxId]) {
        let mut inner = self.inner.write().unwrap();
        for id in tx_ids {
            inner.remove(id);
        }
    }

    fn len(&self) -> usize {
        self.inner.read().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mempool_insert_and_build() {
        let mempool = InMemoryMempool::new();

        let tx1 = vec![1, 2, 3];
        let tx2 = vec![4, 5, 6];

        assert!(mempool.insert(tx1.clone()));
        assert!(mempool.insert(tx2));
        assert!(!mempool.insert(tx1)); // Duplicate

        assert_eq!(mempool.len(), 2);

        let txs = mempool.build(10, &std::collections::BTreeSet::new());
        assert_eq!(txs.len(), 2);
    }

    #[test]
    fn mempool_prune() {
        let mempool = InMemoryMempool::new();

        let tx = vec![1, 2, 3];
        let id = keccak256(&tx);

        mempool.insert(tx);
        assert_eq!(mempool.len(), 1);

        mempool.prune(&[id]);
        assert_eq!(mempool.len(), 0);
    }

    #[test]
    fn mempool_build_with_exclusions() {
        let mempool = InMemoryMempool::new();

        let tx1 = vec![1, 2, 3];
        let tx2 = vec![4, 5, 6];
        let id1 = keccak256(&tx1);

        mempool.insert(tx1);
        mempool.insert(tx2.clone());

        let mut excluded = std::collections::BTreeSet::new();
        excluded.insert(id1);

        let txs = mempool.build(10, &excluded);
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0], tx2);
    }
}
