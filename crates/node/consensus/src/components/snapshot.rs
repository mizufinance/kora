//! In-memory snapshot store implementation.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};

use kora_qmdb::ChangeSet;
use kora_traits::StateDb;

use crate::{
    ConsensusError,
    traits::{Digest, Snapshot, SnapshotStore},
};

/// In-memory snapshot store.
#[derive(Debug)]
pub struct InMemorySnapshotStore<S> {
    snapshots: Arc<RwLock<BTreeMap<Digest, Snapshot<S>>>>,
    persisted: Arc<RwLock<BTreeSet<Digest>>>,
}

impl<S> Clone for InMemorySnapshotStore<S> {
    fn clone(&self) -> Self {
        Self { snapshots: Arc::clone(&self.snapshots), persisted: Arc::clone(&self.persisted) }
    }
}

impl<S> InMemorySnapshotStore<S> {
    /// Create a new empty snapshot store.
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(BTreeMap::new())),
            persisted: Arc::new(RwLock::new(BTreeSet::new())),
        }
    }
}

impl<S> Default for InMemorySnapshotStore<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: StateDb> SnapshotStore<S> for InMemorySnapshotStore<S> {
    fn get(&self, digest: &Digest) -> Option<Snapshot<S>> {
        self.snapshots.read().unwrap().get(digest).cloned()
    }

    fn insert(&self, digest: Digest, snapshot: Snapshot<S>) {
        self.snapshots.write().unwrap().insert(digest, snapshot);
    }

    fn is_persisted(&self, digest: &Digest) -> bool {
        self.persisted.read().unwrap().contains(digest)
    }

    fn mark_persisted(&self, digests: &[Digest]) {
        let mut persisted = self.persisted.write().unwrap();
        for digest in digests {
            persisted.insert(*digest);
        }
    }

    fn merged_changes(
        &self,
        parent: Digest,
        new_changes: ChangeSet,
    ) -> Result<ChangeSet, ConsensusError> {
        let snapshots = self.snapshots.read().unwrap();
        let persisted = self.persisted.read().unwrap();

        // Walk back to find all unpersisted ancestors
        let mut chain = Vec::new();
        let mut current = Some(parent);

        while let Some(digest) = current {
            if persisted.contains(&digest) {
                break;
            }

            let snapshot =
                snapshots.get(&digest).ok_or(ConsensusError::SnapshotNotFound(digest))?;

            chain.push(snapshot.changes.clone());
            current = snapshot.parent;
        }

        // Merge in reverse order (oldest first)
        let mut merged = ChangeSet::new();
        for changes in chain.into_iter().rev() {
            merged.merge(changes);
        }
        merged.merge(new_changes);

        Ok(merged)
    }

    fn changes_for_persist(
        &self,
        digest: Digest,
    ) -> Result<(Vec<Digest>, ChangeSet), ConsensusError> {
        let snapshots = self.snapshots.read().unwrap();
        let persisted = self.persisted.read().unwrap();

        let mut chain = Vec::new();
        let mut changes_chain = Vec::new();
        let mut current = Some(digest);

        while let Some(d) = current {
            if persisted.contains(&d) {
                break;
            }

            let snapshot = snapshots.get(&d).ok_or(ConsensusError::SnapshotNotFound(d))?;

            chain.push(d);
            changes_chain.push(snapshot.changes.clone());
            current = snapshot.parent;
        }

        // Reverse to get oldest-first order
        chain.reverse();
        changes_chain.reverse();

        let mut merged = ChangeSet::new();
        for changes in changes_chain {
            merged.merge(changes);
        }

        Ok((chain, merged))
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;

    use super::*;

    // Mock StateDb for testing
    #[derive(Clone, Debug)]
    struct MockStateDb;

    impl kora_traits::StateDbRead for MockStateDb {
        async fn nonce(
            &self,
            _address: &alloy_primitives::Address,
        ) -> Result<u64, kora_traits::StateDbError> {
            Ok(0)
        }

        async fn balance(
            &self,
            _address: &alloy_primitives::Address,
        ) -> Result<alloy_primitives::U256, kora_traits::StateDbError> {
            Ok(alloy_primitives::U256::ZERO)
        }

        async fn code_hash(
            &self,
            _address: &alloy_primitives::Address,
        ) -> Result<B256, kora_traits::StateDbError> {
            Ok(B256::ZERO)
        }

        async fn code(
            &self,
            _code_hash: &B256,
        ) -> Result<alloy_primitives::Bytes, kora_traits::StateDbError> {
            Ok(alloy_primitives::Bytes::new())
        }

        async fn storage(
            &self,
            _address: &alloy_primitives::Address,
            _slot: &alloy_primitives::U256,
        ) -> Result<alloy_primitives::U256, kora_traits::StateDbError> {
            Ok(alloy_primitives::U256::ZERO)
        }
    }

    impl kora_traits::StateDbWrite for MockStateDb {
        async fn commit(&self, _changes: ChangeSet) -> Result<B256, kora_traits::StateDbError> {
            Ok(B256::ZERO)
        }

        async fn compute_root(
            &self,
            _changes: &ChangeSet,
        ) -> Result<B256, kora_traits::StateDbError> {
            Ok(B256::ZERO)
        }

        fn merge_changes(&self, mut older: ChangeSet, newer: ChangeSet) -> ChangeSet {
            older.merge(newer);
            older
        }
    }

    impl kora_traits::StateDb for MockStateDb {
        async fn state_root(&self) -> Result<B256, kora_traits::StateDbError> {
            Ok(B256::ZERO)
        }
    }

    #[test]
    fn snapshot_store_insert_and_get() {
        let store = InMemorySnapshotStore::<MockStateDb>::new();

        let digest = B256::repeat_byte(0x01);
        let snapshot = Snapshot::new(None, MockStateDb, B256::ZERO, ChangeSet::new());

        assert!(store.get(&digest).is_none());

        store.insert(digest, snapshot);
        assert!(store.get(&digest).is_some());
    }

    #[test]
    fn snapshot_store_persisted() {
        let store = InMemorySnapshotStore::<MockStateDb>::new();

        let digest = B256::repeat_byte(0x01);
        assert!(!store.is_persisted(&digest));

        store.mark_persisted(&[digest]);
        assert!(store.is_persisted(&digest));
    }
}
