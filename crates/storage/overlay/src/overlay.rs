use std::sync::Arc;

use alloy_primitives::{Address, B256, Bytes, U256};
use kora_qmdb::ChangeSet;
use kora_traits::{StateDb, StateDbError, StateDbRead, StateDbWrite};

/// State overlay that layers pending changes on top of a base state database.
#[derive(Clone, Debug)]
pub struct OverlayState<S> {
    base: S,
    changes: Arc<ChangeSet>,
}

impl<S> OverlayState<S> {
    /// Create a new overlay from a base state and a change set.
    pub fn new(base: S, changes: ChangeSet) -> Self {
        Self { base, changes: Arc::new(changes) }
    }

    /// Merge the current overlay changes with a newer change set.
    pub fn merge_changes(&self, newer: ChangeSet) -> ChangeSet {
        let mut merged = (*self.changes).clone();
        merged.merge(newer);
        merged
    }
}

impl<S: Clone> OverlayState<S> {
    /// Return a clone of the base state handle.
    pub fn base(&self) -> S {
        self.base.clone()
    }
}

impl<S: StateDbRead> StateDbRead for OverlayState<S> {
    fn nonce(
        &self,
        address: &Address,
    ) -> impl std::future::Future<Output = Result<u64, StateDbError>> + Send {
        let address = *address;
        let base = self.base.clone();
        let changes = Arc::clone(&self.changes);
        async move {
            if let Some(update) = changes.accounts.get(&address) {
                return Ok(update.nonce);
            }
            base.nonce(&address).await
        }
    }

    fn balance(
        &self,
        address: &Address,
    ) -> impl std::future::Future<Output = Result<U256, StateDbError>> + Send {
        let address = *address;
        let base = self.base.clone();
        let changes = Arc::clone(&self.changes);
        async move {
            if let Some(update) = changes.accounts.get(&address) {
                return Ok(update.balance);
            }
            base.balance(&address).await
        }
    }

    fn code_hash(
        &self,
        address: &Address,
    ) -> impl std::future::Future<Output = Result<B256, StateDbError>> + Send {
        let address = *address;
        let base = self.base.clone();
        let changes = Arc::clone(&self.changes);
        async move {
            if let Some(update) = changes.accounts.get(&address) {
                return Ok(update.code_hash);
            }
            base.code_hash(&address).await
        }
    }

    fn code(
        &self,
        code_hash: &B256,
    ) -> impl std::future::Future<Output = Result<Bytes, StateDbError>> + Send {
        let code_hash = *code_hash;
        let base = self.base.clone();
        let changes = Arc::clone(&self.changes);
        async move {
            for update in changes.accounts.values() {
                if update.code_hash == code_hash
                    && let Some(code) = &update.code
                {
                    return Ok(Bytes::from(code.clone()));
                }
            }
            base.code(&code_hash).await
        }
    }

    fn storage(
        &self,
        address: &Address,
        slot: &U256,
    ) -> impl std::future::Future<Output = Result<U256, StateDbError>> + Send {
        let address = *address;
        let slot = *slot;
        let base = self.base.clone();
        let changes = Arc::clone(&self.changes);
        async move {
            if let Some(update) = changes.accounts.get(&address) {
                if update.selfdestructed {
                    return Ok(U256::ZERO);
                }
                if let Some(value) = update.storage.get(&slot) {
                    return Ok(*value);
                }
                if update.created {
                    return Ok(U256::ZERO);
                }
            }
            base.storage(&address, &slot).await
        }
    }
}

impl<S: StateDbWrite> StateDbWrite for OverlayState<S> {
    fn commit(
        &self,
        changes: ChangeSet,
    ) -> impl std::future::Future<Output = Result<B256, StateDbError>> + Send {
        let base = self.base.clone();
        let overlay = Arc::clone(&self.changes);
        async move {
            let mut merged = (*overlay).clone();
            merged.merge(changes);
            base.commit(merged).await
        }
    }

    fn compute_root(
        &self,
        changes: &ChangeSet,
    ) -> impl std::future::Future<Output = Result<B256, StateDbError>> + Send {
        let base = self.base.clone();
        let overlay = Arc::clone(&self.changes);
        let changes = changes.clone();
        async move {
            let mut merged = (*overlay).clone();
            merged.merge(changes);
            base.compute_root(&merged).await
        }
    }

    fn merge_changes(&self, older: ChangeSet, newer: ChangeSet) -> ChangeSet {
        self.base.merge_changes(older, newer)
    }
}

impl<S: StateDb> StateDb for OverlayState<S> {
    fn state_root(&self) -> impl std::future::Future<Output = Result<B256, StateDbError>> + Send {
        let base = self.base.clone();
        async move { base.state_root().await }
    }
}
