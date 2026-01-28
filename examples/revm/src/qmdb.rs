//! QMDB-backed storage adapter for the REVM example, powered by kora storage crates.

use std::sync::Arc;

use alloy_evm::revm::database::CacheDB;
use kora_backend::{AccountStore, CodeStore, CommonwareBackend, CommonwareRootProvider, StorageStore};
use kora_domain::StateRoot;
use kora_handlers::{HandleError, QmdbHandle, QmdbRefDb as HandlerQmdbRefDb};
use kora_traits::{StateDb, StateDbWrite};
use thiserror::Error;
use tokio::sync::RwLock;

pub(crate) use kora_backend::QmdbBackendConfig as QmdbConfig;
pub(crate) use kora_qmdb::{AccountUpdate, ChangeSet as QmdbChangeSet};

type Handle = QmdbHandle<AccountStore, StorageStore, CodeStore>;
type QmdbRefDb = HandlerQmdbRefDb<AccountStore, StorageStore, CodeStore>;

/// Execution database type used by the REVM example.
pub(crate) type RevmDb = CacheDB<QmdbRefDb>;

/// QMDB ledger service backed by kora storage crates.
#[derive(Clone)]
pub(crate) struct QmdbLedger {
    handle: Handle,
}

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("backend error: {0}")]
    Backend(#[from] kora_backend::BackendError),
    #[error("handler error: {0}")]
    Handler(#[from] HandleError),
    #[error("state db error: {0}")]
    StateDb(#[from] kora_traits::StateDbError),
    #[error("missing tokio runtime for async db bridge")]
    MissingRuntime,
}

impl QmdbLedger {
    /// Initializes the QMDB partitions and populates the genesis allocation.
    pub(crate) async fn init(
        context: commonware_runtime::tokio::Context,
        config: QmdbConfig,
        genesis_alloc: Vec<(alloy_evm::revm::primitives::Address, alloy_evm::revm::primitives::U256)>,
    ) -> Result<Self, Error> {
        let backend = CommonwareBackend::open(context.clone(), config.clone()).await?;
        let root_provider =
            CommonwareRootProvider::new(context, config);
        let (accounts, storage, code) = backend.into_stores();
        let handle = Handle::new(accounts, storage, code)
            .with_root_provider(Arc::new(RwLock::new(root_provider)));
        handle.init_genesis(genesis_alloc).await?;
        Ok(Self { handle })
    }

    /// Exposes a synchronous REVM database view backed by QMDB.
    pub(crate) fn database(&self) -> Result<QmdbRefDb, Error> {
        QmdbRefDb::new(self.handle.clone()).ok_or(Error::MissingRuntime)
    }

    /// Computes the root for a change set without committing.
    pub(crate) async fn compute_root(&self, changes: QmdbChangeSet) -> Result<StateRoot, Error> {
        let root = StateDbWrite::compute_root(&self.handle, &changes).await?;
        Ok(StateRoot(root))
    }

    /// Commits the provided changes to QMDB and returns the resulting root.
    pub(crate) async fn commit_changes(&self, changes: QmdbChangeSet) -> Result<StateRoot, Error> {
        let root = StateDbWrite::commit(&self.handle, changes).await?;
        Ok(StateRoot(root))
    }

    /// Returns the current authenticated root stored in QMDB.
    pub(crate) async fn root(&self) -> Result<StateRoot, Error> {
        let root = StateDb::state_root(&self.handle).await?;
        Ok(StateRoot(root))
    }
}
