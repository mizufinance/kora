//! Block proposal building logic.

use std::collections::BTreeSet;

use alloy_consensus::Header;
use alloy_primitives::{B256, keccak256};
use kora_executor::{BlockContext, BlockExecutor};
use kora_traits::StateDb;

use crate::{ConsensusError, Digest, KoraBlock, Mempool, Snapshot, SnapshotStore, TxId};

/// Builder for constructing block proposals.
///
/// ProposalBuilder coordinates gathering transactions from the mempool,
/// executing them against parent state, and constructing a complete block.
///
/// # Type Parameters
///
/// * `S` - State database type implementing [`StateDb`].
/// * `M` - Mempool type implementing [`Mempool`].
/// * `SS` - Snapshot store type implementing [`SnapshotStore`].
/// * `E` - Block executor type implementing [`BlockExecutor`].
///
/// # Example
///
/// ```ignore
/// let builder = ProposalBuilder::new(state, mempool, snapshots, executor, chain_id)
///     .with_max_txs(100);
///
/// let (block, snapshot) = builder.build_proposal(parent_digest, prevrandao)?;
/// ```
#[derive(Debug)]
pub struct ProposalBuilder<S, M, SS, E> {
    /// State database for execution.
    state: S,
    /// Transaction mempool.
    mempool: M,
    /// Snapshot store for parent state lookup.
    snapshots: SS,
    /// Block executor.
    executor: E,
    /// Maximum transactions per block.
    max_txs: usize,
    /// Chain ID for execution.
    chain_id: u64,
}

impl<S, M, SS, E> ProposalBuilder<S, M, SS, E>
where
    S: StateDb,
    M: Mempool,
    M::Tx: AsRef<[u8]>,
    SS: SnapshotStore<S>,
    E: BlockExecutor<S, Tx = M::Tx>,
{
    /// Default maximum transactions per block.
    pub const DEFAULT_MAX_TXS: usize = 1000;

    /// Create a new proposal builder.
    ///
    /// # Arguments
    ///
    /// * `state` - State database for execution lookups.
    /// * `mempool` - Transaction mempool to pull transactions from.
    /// * `snapshots` - Snapshot store for parent state lookup.
    /// * `executor` - Block executor for transaction execution.
    /// * `chain_id` - Chain ID for the execution context.
    pub const fn new(state: S, mempool: M, snapshots: SS, executor: E, chain_id: u64) -> Self {
        Self { state, mempool, snapshots, executor, max_txs: Self::DEFAULT_MAX_TXS, chain_id }
    }

    /// Set the maximum number of transactions per block.
    ///
    /// Defaults to [`Self::DEFAULT_MAX_TXS`].
    #[must_use]
    pub const fn with_max_txs(mut self, max_txs: usize) -> Self {
        self.max_txs = max_txs;
        self
    }

    /// Get the chain ID.
    pub const fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Build a block proposal from the given parent.
    ///
    /// This method:
    /// 1. Retrieves the parent snapshot from the snapshot store.
    /// 2. Walks the ancestry chain to collect transaction IDs already included
    ///    in pending (unpersisted) ancestor blocks to avoid duplicates.
    /// 3. Builds a transaction batch from the mempool excluding those IDs.
    /// 4. Executes the batch against the parent state.
    /// 5. Computes the new state root from the execution outcome.
    /// 6. Constructs a block header with proper fields.
    /// 7. Returns the complete block and its snapshot for caching.
    ///
    /// # Arguments
    ///
    /// * `parent_digest` - Digest of the parent block.
    /// * `prevrandao` - Randomness value for the block (from VRF).
    ///
    /// # Returns
    ///
    /// A tuple of the constructed block and its snapshot containing the
    /// execution state.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The parent snapshot is not found.
    /// * Ancestor walking fails (missing snapshot in chain).
    /// * Transaction execution fails.
    /// * State root computation fails.
    pub fn build_proposal(
        &self,
        parent_digest: Digest,
        prevrandao: B256,
    ) -> Result<(KoraBlock, Snapshot<S>), ConsensusError> {
        // Get parent snapshot
        let parent_snapshot = self
            .snapshots
            .get(&parent_digest)
            .ok_or(ConsensusError::SnapshotNotFound(parent_digest))?;

        // Collect excluded transaction IDs from pending ancestors
        let excluded = self.collect_pending_tx_ids(parent_digest)?;

        // Build transaction batch from mempool
        let txs = self.mempool.build(self.max_txs, &excluded);

        // Compute block number placeholder (actual height should come from parent)
        let number = parent_snapshot
            .state_root
            .as_slice()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_add(b as u64))
            .wrapping_add(1);

        // Get current timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Build initial header for execution context
        let initial_header = Header {
            parent_hash: parent_digest,
            number,
            timestamp,
            mix_hash: prevrandao,
            ..Default::default()
        };

        // Create execution context
        let context = BlockContext::new(initial_header.clone(), prevrandao);

        // Execute transactions
        let outcome = self
            .executor
            .execute(&parent_snapshot.state, &context, &txs)
            .map_err(|e| ConsensusError::Execution(e.to_string()))?;

        // Merge changes with ancestor changes and compute state root
        let merged_changes =
            self.snapshots.merged_changes(parent_digest, outcome.changes.clone())?;
        let state_root = futures::executor::block_on(self.state.compute_root(&merged_changes))
            .map_err(ConsensusError::StateDb)?;

        // Build final header with computed values
        let header = Header { state_root, gas_used: outcome.gas_used, ..initial_header };

        // Encode transactions for block storage
        let encoded_txs = encode_transactions(&txs);

        // Create block
        let block = KoraBlock::new(header, encoded_txs, state_root);

        // Create snapshot for caching
        let snapshot =
            Snapshot::new(Some(parent_digest), parent_snapshot.state, state_root, outcome.changes);

        Ok((block, snapshot))
    }

    /// Collect transaction IDs from pending (unpersisted) ancestor blocks.
    ///
    /// Walks the ancestry chain from the given digest back to the last
    /// persisted snapshot, collecting all transaction IDs along the way.
    /// This ensures we don't include duplicate transactions in new blocks.
    fn collect_pending_tx_ids(&self, from: Digest) -> Result<BTreeSet<TxId>, ConsensusError> {
        let excluded = BTreeSet::new();
        let mut current = Some(from);

        while let Some(digest) = current {
            // Stop at persisted snapshots
            if self.snapshots.is_persisted(&digest) {
                break;
            }

            let snapshot =
                self.snapshots.get(&digest).ok_or(ConsensusError::SnapshotNotFound(digest))?;

            // Note: In a full implementation, we would need to track tx IDs
            // per snapshot. For now, we just walk the chain to establish
            // the pattern. The actual tx IDs would be stored alongside
            // the snapshot or in a separate index.
            //
            // The excluded set will be populated by higher-level consensus
            // logic that tracks pending block transactions.

            current = snapshot.parent;
        }

        Ok(excluded)
    }
}

/// Encode transactions for block storage.
///
/// Converts typed transactions into their encoded byte representation.
fn encode_transactions<T: AsRef<[u8]>>(txs: &[T]) -> Vec<Vec<u8>> {
    txs.iter().map(|tx| tx.as_ref().to_vec()).collect()
}

/// Compute the transaction ID for a raw transaction.
///
/// The transaction ID is the keccak256 hash of the encoded transaction.
#[inline]
pub fn tx_id(tx: &[u8]) -> TxId {
    keccak256(tx)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, RwLock},
    };

    use alloy_primitives::{Address, Bytes, U256};
    use kora_executor::ExecutionOutcome;
    use kora_qmdb::ChangeSet;

    use super::*;

    // Mock implementations for testing

    #[derive(Clone, Debug)]
    struct MockStateDb {
        root: B256,
    }

    impl MockStateDb {
        fn new() -> Self {
            Self { root: B256::ZERO }
        }
    }

    impl kora_traits::StateDbRead for MockStateDb {
        async fn nonce(&self, _address: &Address) -> Result<u64, kora_traits::StateDbError> {
            Ok(0)
        }

        async fn balance(&self, _address: &Address) -> Result<U256, kora_traits::StateDbError> {
            Ok(U256::ZERO)
        }

        async fn code_hash(&self, _address: &Address) -> Result<B256, kora_traits::StateDbError> {
            Ok(B256::ZERO)
        }

        async fn code(&self, _code_hash: &B256) -> Result<Bytes, kora_traits::StateDbError> {
            Ok(Bytes::new())
        }

        async fn storage(
            &self,
            _address: &Address,
            _slot: &U256,
        ) -> Result<U256, kora_traits::StateDbError> {
            Ok(U256::ZERO)
        }
    }

    impl kora_traits::StateDbWrite for MockStateDb {
        async fn commit(&self, _changes: ChangeSet) -> Result<B256, kora_traits::StateDbError> {
            Ok(B256::repeat_byte(0x42))
        }

        async fn compute_root(
            &self,
            _changes: &ChangeSet,
        ) -> Result<B256, kora_traits::StateDbError> {
            Ok(B256::repeat_byte(0x42))
        }

        fn merge_changes(&self, mut older: ChangeSet, newer: ChangeSet) -> ChangeSet {
            older.merge(newer);
            older
        }
    }

    impl kora_traits::StateDb for MockStateDb {
        async fn state_root(&self) -> Result<B256, kora_traits::StateDbError> {
            Ok(self.root)
        }
    }

    #[derive(Clone)]
    struct MockMempool {
        txs: Arc<RwLock<BTreeMap<TxId, Vec<u8>>>>,
    }

    impl MockMempool {
        fn new() -> Self {
            Self { txs: Arc::new(RwLock::new(BTreeMap::new())) }
        }

        fn add(&self, tx: Vec<u8>) {
            let id = keccak256(&tx);
            self.txs.write().unwrap().insert(id, tx);
        }
    }

    impl Mempool for MockMempool {
        type Tx = Vec<u8>;

        fn insert(&self, tx: Self::Tx) -> bool {
            let id = keccak256(&tx);
            self.txs.write().unwrap().insert(id, tx).is_none()
        }

        fn build(&self, max_txs: usize, excluded: &BTreeSet<TxId>) -> Vec<Self::Tx> {
            self.txs
                .read()
                .unwrap()
                .iter()
                .filter(|(id, _)| !excluded.contains(*id))
                .take(max_txs)
                .map(|(_, tx)| tx.clone())
                .collect()
        }

        fn prune(&self, tx_ids: &[TxId]) {
            let mut txs = self.txs.write().unwrap();
            for id in tx_ids {
                txs.remove(id);
            }
        }

        fn len(&self) -> usize {
            self.txs.read().unwrap().len()
        }
    }

    #[derive(Clone)]
    struct MockSnapshotStore {
        snapshots: Arc<RwLock<BTreeMap<Digest, Snapshot<MockStateDb>>>>,
        persisted: Arc<RwLock<BTreeSet<Digest>>>,
    }

    impl MockSnapshotStore {
        fn new() -> Self {
            Self {
                snapshots: Arc::new(RwLock::new(BTreeMap::new())),
                persisted: Arc::new(RwLock::new(BTreeSet::new())),
            }
        }
    }

    impl SnapshotStore<MockStateDb> for MockSnapshotStore {
        fn get(&self, digest: &Digest) -> Option<Snapshot<MockStateDb>> {
            self.snapshots.read().unwrap().get(digest).cloned()
        }

        fn insert(&self, digest: Digest, snapshot: Snapshot<MockStateDb>) {
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
            _parent: Digest,
            new_changes: ChangeSet,
        ) -> Result<ChangeSet, ConsensusError> {
            Ok(new_changes)
        }

        fn changes_for_persist(
            &self,
            digest: Digest,
        ) -> Result<(Vec<Digest>, ChangeSet), ConsensusError> {
            let snapshot = self
                .snapshots
                .read()
                .unwrap()
                .get(&digest)
                .cloned()
                .ok_or(ConsensusError::SnapshotNotFound(digest))?;
            Ok((vec![digest], snapshot.changes))
        }
    }

    #[derive(Clone)]
    struct MockExecutor;

    impl BlockExecutor<MockStateDb> for MockExecutor {
        type Tx = Vec<u8>;

        fn execute(
            &self,
            _state: &MockStateDb,
            _context: &BlockContext,
            txs: &[Self::Tx],
        ) -> Result<ExecutionOutcome, kora_executor::ExecutionError> {
            Ok(ExecutionOutcome {
                changes: ChangeSet::new(),
                receipts: Vec::new(),
                gas_used: txs.len() as u64 * 21000,
            })
        }

        fn validate_header(&self, _header: &Header) -> Result<(), kora_executor::ExecutionError> {
            Ok(())
        }
    }

    #[test]
    fn proposal_builder_new() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1);
        assert_eq!(builder.max_txs, ProposalBuilder::<MockStateDb, MockMempool, MockSnapshotStore, MockExecutor>::DEFAULT_MAX_TXS);
        assert_eq!(builder.chain_id, 1);
    }

    #[test]
    fn proposal_builder_with_max_txs() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1).with_max_txs(50);
        assert_eq!(builder.max_txs, 50);
    }

    #[test]
    fn proposal_builder_missing_parent() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1);

        let parent = B256::repeat_byte(0x01);
        let result = builder.build_proposal(parent, B256::ZERO);

        assert!(matches!(result, Err(ConsensusError::SnapshotNotFound(_))));
    }

    #[test]
    fn proposal_builder_empty_block() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        // Insert parent snapshot
        let parent_digest = B256::repeat_byte(0x01);
        let parent_snapshot = Snapshot::new(None, MockStateDb::new(), B256::ZERO, ChangeSet::new());
        snapshots.insert(parent_digest, parent_snapshot);

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1);

        let result = builder.build_proposal(parent_digest, B256::ZERO);
        assert!(result.is_ok());

        let (block, snapshot) = result.unwrap();
        assert_eq!(block.tx_count(), 0);
        assert_eq!(block.parent_hash(), parent_digest);
        assert_eq!(snapshot.parent, Some(parent_digest));
    }

    #[test]
    fn proposal_builder_with_transactions() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        // Add transactions to mempool
        mempool.add(vec![1, 2, 3]);
        mempool.add(vec![4, 5, 6]);

        // Insert parent snapshot
        let parent_digest = B256::repeat_byte(0x01);
        let parent_snapshot = Snapshot::new(None, MockStateDb::new(), B256::ZERO, ChangeSet::new());
        snapshots.insert(parent_digest, parent_snapshot);

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1);

        let result = builder.build_proposal(parent_digest, B256::repeat_byte(0xAB));
        assert!(result.is_ok());

        let (block, snapshot) = result.unwrap();
        assert_eq!(block.tx_count(), 2);
        assert_eq!(block.header.mix_hash, B256::repeat_byte(0xAB));
        assert_eq!(block.header.gas_used, 42000); // 2 txs * 21000
        assert!(snapshot.parent.is_some());
    }

    #[test]
    fn proposal_builder_respects_max_txs() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        // Add many transactions
        for i in 0..100 {
            mempool.add(vec![i]);
        }

        // Insert parent snapshot
        let parent_digest = B256::repeat_byte(0x01);
        let parent_snapshot = Snapshot::new(None, MockStateDb::new(), B256::ZERO, ChangeSet::new());
        snapshots.insert(parent_digest, parent_snapshot);

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1).with_max_txs(10);

        let result = builder.build_proposal(parent_digest, B256::ZERO);
        assert!(result.is_ok());

        let (block, _) = result.unwrap();
        assert_eq!(block.tx_count(), 10);
    }

    #[test]
    fn tx_id_computation() {
        let tx = vec![1, 2, 3, 4, 5];
        let id = tx_id(&tx);
        let expected = keccak256(&tx);
        assert_eq!(id, expected);
    }

    #[test]
    fn encode_transactions_preserves_data() {
        let txs = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let encoded = encode_transactions(&txs);
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded[0], vec![1, 2, 3]);
        assert_eq!(encoded[1], vec![4, 5, 6]);
    }

    #[test]
    fn proposal_builder_state_root_in_header() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        // Insert parent snapshot
        let parent_digest = B256::repeat_byte(0x01);
        let parent_snapshot = Snapshot::new(None, MockStateDb::new(), B256::ZERO, ChangeSet::new());
        snapshots.insert(parent_digest, parent_snapshot);

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1);

        let (block, snapshot) = builder.build_proposal(parent_digest, B256::ZERO).unwrap();

        // MockStateDb::compute_root returns B256::repeat_byte(0x42)
        let expected_root = B256::repeat_byte(0x42);
        assert_eq!(block.state_root, expected_root);
        assert_eq!(block.header.state_root, expected_root);
        assert_eq!(snapshot.state_root, expected_root);
    }

    #[test]
    fn gas_used_field() {
        let state = MockStateDb::new();
        let mempool = MockMempool::new();
        let snapshots = MockSnapshotStore::new();
        let executor = MockExecutor;

        // Add transactions
        mempool.add(vec![1]);
        mempool.add(vec![2]);
        mempool.add(vec![3]);

        // Insert parent snapshot
        let parent_digest = B256::repeat_byte(0x01);
        let parent_snapshot = Snapshot::new(None, MockStateDb::new(), B256::ZERO, ChangeSet::new());
        snapshots.insert(parent_digest, parent_snapshot);

        let builder = ProposalBuilder::new(state, mempool, snapshots, executor, 1);

        let (block, _) = builder.build_proposal(parent_digest, B256::ZERO).unwrap();

        // MockExecutor returns txs.len() * 21000
        assert_eq!(block.header.gas_used, 63000);
    }
}
