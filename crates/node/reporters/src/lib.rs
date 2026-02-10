//! Consensus reporters for Kora nodes.
#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/refcell/kora/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use std::{fmt, marker::PhantomData, sync::Arc};

use alloy_consensus::{Transaction as _, TxEnvelope, transaction::SignerRecoverable as _};
use alloy_eips::eip2718::Decodable2718 as _;
use alloy_primitives::{B256, Bytes, keccak256};
use commonware_consensus::{
    Block as _, Reporter,
    marshal::Update,
    simplex::{
        scheme::bls12381_threshold::{Scheme, Seedable as _},
        types::Activity,
    },
};
use commonware_cryptography::{Committable as _, bls12381::primitives::variant::Variant};
use commonware_runtime::{Spawner as _, tokio};
use commonware_utils::acknowledgement::Acknowledgement as _;
use kora_consensus::BlockExecution;
use kora_domain::{Block, ConsensusDigest, PublicKey};
use kora_executor::{BlockContext, BlockExecutor, ExecutionReceipt};
use kora_indexer::{BlockIndex, IndexedBlock, IndexedLog, IndexedReceipt, IndexedTransaction};
use kora_ledger::LedgerService;
use kora_overlay::OverlayState;
use kora_qmdb_ledger::QmdbState;
use kora_rpc::NodeState;
use tracing::{error, trace, warn};

/// Provides block execution context for finalized block verification.
pub trait BlockContextProvider: Clone + Send + Sync + 'static {
    /// Build a block execution context for the provided block.
    fn context(&self, block: &Block) -> BlockContext;
}

/// Helper function for SeedReporter::report that owns all its inputs.
async fn seed_report_inner<V: Variant>(
    state: LedgerService,
    activity: Activity<Scheme<PublicKey, V>, ConsensusDigest>,
) {
    match activity {
        Activity::Notarization(notarization) => {
            state
                .set_seed(
                    notarization.proposal.payload,
                    SeedReporter::<V>::hash_seed(notarization.seed()),
                )
                .await;
        }
        Activity::Finalization(finalization) => {
            state
                .set_seed(
                    finalization.proposal.payload,
                    SeedReporter::<V>::hash_seed(finalization.seed()),
                )
                .await;
        }
        _ => {}
    }
}

#[derive(Clone)]
/// Tracks simplex activity to store seed hashes for future proposals.
pub struct SeedReporter<V> {
    /// Ledger service that keeps per-digest seeds and snapshots.
    state: LedgerService,
    /// Marker indicating the variant for the threshold scheme in use.
    _variant: PhantomData<V>,
}

impl<V> fmt::Debug for SeedReporter<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeedReporter").finish_non_exhaustive()
    }
}

impl<V> SeedReporter<V> {
    /// Create a new seed reporter for the provided ledger service.
    pub const fn new(state: LedgerService) -> Self {
        Self { state, _variant: PhantomData }
    }

    fn hash_seed(seed: impl commonware_codec::Encode) -> B256 {
        keccak256(seed.encode())
    }
}

impl<V> Reporter for SeedReporter<V>
where
    V: Variant,
{
    type Activity = Activity<Scheme<PublicKey, V>, ConsensusDigest>;

    fn report(&mut self, activity: Self::Activity) -> impl std::future::Future<Output = ()> + Send {
        let state = self.state.clone();
        async move {
            seed_report_inner(state, activity).await;
        }
    }
}

async fn handle_finalized_update<E, P>(
    state: LedgerService,
    context: tokio::Context,
    executor: E,
    provider: P,
    update: Update<Block>,
    block_index: Option<Arc<BlockIndex>>,
) where
    E: BlockExecutor<OverlayState<QmdbState>, Tx = Bytes>,
    P: BlockContextProvider,
{
    match update {
        Update::Tip(..) => {}
        Update::Block(block, ack) => {
            let digest = block.commitment();
            let mut cached_receipts: Option<(Vec<ExecutionReceipt>, u64)> = None;

            if state.query_state_root(digest).await.is_none() {
                trace!(?digest, "missing snapshot for finalized block; re-executing");
                let parent_digest = block.parent();
                let Some(parent_snapshot) = state.parent_snapshot(parent_digest).await else {
                    error!(?digest, ?parent_digest, "missing parent snapshot for finalized block");
                    ack.acknowledge();
                    return;
                };
                let block_context = provider.context(&block);
                let execution = match BlockExecution::execute(
                    &parent_snapshot,
                    &executor,
                    &block_context,
                    &block.txs,
                )
                .await
                {
                    Ok(result) => result,
                    Err(err) => {
                        error!(?digest, error = ?err, "failed to execute finalized block");
                        ack.acknowledge();
                        return;
                    }
                };
                let merged_changes =
                    parent_snapshot.state.merge_changes(execution.outcome.changes.clone());
                let state_root = match state
                    .compute_root_from_store(parent_digest, execution.outcome.changes.clone())
                    .await
                {
                    Ok(root) => root,
                    Err(err) => {
                        error!(?digest, error = ?err, "failed to compute qmdb root");
                        ack.acknowledge();
                        return;
                    }
                };
                if state_root != block.state_root {
                    warn!(
                        ?digest,
                        expected = ?block.state_root,
                        computed = ?state_root,
                        "state root mismatch for finalized block"
                    );
                    ack.acknowledge();
                    return;
                }
                // Save receipts for indexing before consuming changes.
                let receipts = execution.outcome.receipts;
                let gas_used = execution.outcome.gas_used;
                cached_receipts = Some((receipts, gas_used));

                let next_state = OverlayState::new(parent_snapshot.state.base(), merged_changes);
                state
                    .insert_snapshot(
                        digest,
                        parent_digest,
                        next_state,
                        state_root,
                        execution.outcome.changes,
                        &block.txs,
                    )
                    .await;
            } else {
                trace!(?digest, "using cached snapshot for finalized block");
            }
            let persist_state = state.clone();
            let persist_handle = context
                .shared(true)
                .spawn(move |_| async move { persist_state.persist_snapshot(digest).await });
            let persist_result = match persist_handle.await {
                Ok(result) => result,
                Err(err) => {
                    error!(?digest, error = ?err, "persist task failed");
                    ack.acknowledge();
                    return;
                }
            };
            if let Err(err) = persist_result {
                error!(?digest, error = ?err, "failed to persist finalized block");
                ack.acknowledge();
                return;
            }

            // Index the finalized block if a block index is present.
            if let Some(ref index) = block_index {
                let (receipts, gas_used) = match cached_receipts {
                    Some(cached) => cached,
                    None => {
                        // Cached-snapshot path: re-execute to get receipts for indexing.
                        match re_execute_for_receipts(&state, &executor, &provider, &block).await {
                            Some(result) => result,
                            None => {
                                warn!(
                                    height = block.height,
                                    "failed to obtain receipts for indexing"
                                );
                                (Vec::new(), 0)
                            }
                        }
                    }
                };
                index_finalized_block(index, &block, &provider, &receipts, gas_used);
            }

            state.prune_mempool(&block.txs).await;
            // Marshal waits for the application to acknowledge processing before advancing the
            // delivery floor. Without this, the node can stall on finalized block delivery.
            ack.acknowledge();
        }
    }
}

/// Re-execute a finalized block to obtain receipts for indexing.
async fn re_execute_for_receipts<E, P>(
    state: &LedgerService,
    executor: &E,
    provider: &P,
    block: &Block,
) -> Option<(Vec<ExecutionReceipt>, u64)>
where
    E: BlockExecutor<OverlayState<QmdbState>, Tx = Bytes>,
    P: BlockContextProvider,
{
    let parent_digest = block.parent();
    let parent_snapshot = state.parent_snapshot(parent_digest).await?;
    let block_context = provider.context(block);
    let execution =
        BlockExecution::execute(&parent_snapshot, executor, &block_context, &block.txs).await.ok()?;
    Some((execution.outcome.receipts, execution.outcome.gas_used))
}

/// Index a finalized block into the block index.
fn index_finalized_block<P: BlockContextProvider>(
    index: &BlockIndex,
    block: &Block,
    provider: &P,
    receipts: &[ExecutionReceipt],
    gas_used: u64,
) {
    let block_hash = block.id().0;
    let block_context = provider.context(block);

    let mut tx_hashes = Vec::with_capacity(block.txs.len());
    let mut indexed_txs = Vec::with_capacity(block.txs.len());
    let mut indexed_receipts = Vec::with_capacity(receipts.len());

    for (i, tx) in block.txs.iter().enumerate() {
        let tx_hash = keccak256(&tx.bytes);

        let Ok(envelope) = TxEnvelope::decode_2718(&mut tx.bytes.as_ref()) else {
            warn!(height = block.height, index = i, "failed to decode tx for indexing");
            tx_hashes.push(tx_hash);
            continue;
        };
        let Ok(sender) = envelope.recover_signer() else {
            warn!(height = block.height, index = i, "failed to recover sender for indexing");
            tx_hashes.push(tx_hash);
            continue;
        };

        tx_hashes.push(tx_hash);

        indexed_txs.push(IndexedTransaction {
            hash: tx_hash,
            block_hash,
            block_number: block.height,
            index: i as u64,
            from: sender,
            to: envelope.to(),
            value: envelope.value(),
            gas_limit: envelope.gas_limit(),
            gas_price: envelope.gas_price().unwrap_or_else(|| envelope.max_fee_per_gas()),
            input: envelope.input().clone(),
            nonce: envelope.nonce(),
        });

        if let Some(receipt) = receipts.get(i) {
            let logs = receipt
                .logs()
                .iter()
                .enumerate()
                .map(|(li, log)| IndexedLog {
                    address: log.address,
                    topics: log.data.topics().to_vec(),
                    data: log.data.data.clone(),
                    log_index: li as u64,
                })
                .collect();

            indexed_receipts.push(IndexedReceipt {
                transaction_hash: tx_hash,
                block_hash,
                block_number: block.height,
                transaction_index: i as u64,
                from: sender,
                to: envelope.to(),
                cumulative_gas_used: receipt.cumulative_gas_used(),
                gas_used: receipt.gas_used,
                contract_address: receipt.contract_address,
                logs,
                status: receipt.success(),
            });
        }
    }

    let indexed_block = IndexedBlock {
        hash: block_hash,
        number: block.height,
        parent_hash: block.parent.0,
        state_root: block.state_root.0,
        timestamp: block.height,
        gas_limit: block_context.header.gas_limit,
        gas_used,
        base_fee_per_gas: block_context.header.base_fee_per_gas,
        transaction_hashes: tx_hashes,
    };

    index.insert_block(indexed_block, indexed_txs, indexed_receipts);
    trace!(height = block.height, "indexed finalized block");
}

#[derive(Clone)]
/// Persists finalized blocks.
pub struct FinalizedReporter<E, P> {
    /// Ledger service used to verify blocks and persist snapshots.
    state: LedgerService,
    /// Tokio context used to schedule blocking work.
    context: tokio::Context,
    /// Block executor used to replay finalized blocks.
    executor: E,
    /// Provider that builds block execution context.
    provider: P,
    /// Optional block index for indexing finalized blocks for RPC queries.
    block_index: Option<Arc<BlockIndex>>,
}

impl<E, P> fmt::Debug for FinalizedReporter<E, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FinalizedReporter").finish_non_exhaustive()
    }
}

impl<E, P> FinalizedReporter<E, P>
where
    E: BlockExecutor<OverlayState<QmdbState>, Tx = Bytes>,
    P: BlockContextProvider,
{
    /// Create a new finalized reporter.
    pub const fn new(
        state: LedgerService,
        context: tokio::Context,
        executor: E,
        provider: P,
    ) -> Self {
        Self { state, context, executor, provider, block_index: None }
    }

    /// Set the block index for indexing finalized blocks.
    #[must_use]
    pub fn with_block_index(mut self, index: Arc<BlockIndex>) -> Self {
        self.block_index = Some(index);
        self
    }
}

impl<E, P> Reporter for FinalizedReporter<E, P>
where
    E: BlockExecutor<OverlayState<QmdbState>, Tx = Bytes>,
    P: BlockContextProvider,
{
    type Activity = Update<Block>;

    fn report(&mut self, update: Self::Activity) -> impl std::future::Future<Output = ()> + Send {
        let state = self.state.clone();
        let context = self.context.clone();
        let executor = self.executor.clone();
        let provider = self.provider.clone();
        let block_index = self.block_index.clone();
        async move {
            handle_finalized_update(state, context, executor, provider, update, block_index).await;
        }
    }
}

/// Reporter that updates RPC-visible node state from consensus activity.
///
/// This reporter tracks:
/// - Current view number (from notarizations)
/// - Finalized block count
/// - Nullified round count
#[derive(Clone)]
pub struct NodeStateReporter<S> {
    /// RPC node state to update.
    state: NodeState,
    /// Marker for the signing scheme.
    _scheme: PhantomData<S>,
}

impl<S> fmt::Debug for NodeStateReporter<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeStateReporter").finish_non_exhaustive()
    }
}

impl<S> NodeStateReporter<S> {
    /// Create a new node state reporter.
    pub const fn new(state: NodeState) -> Self {
        Self { state, _scheme: PhantomData }
    }
}

impl<S> Reporter for NodeStateReporter<S>
where
    S: commonware_cryptography::certificate::Scheme + Clone + Send + 'static,
{
    type Activity = Activity<S, ConsensusDigest>;

    fn report(&mut self, activity: Self::Activity) -> impl std::future::Future<Output = ()> + Send {
        match &activity {
            Activity::Notarization(n) => {
                self.state.set_view(n.proposal.round.view().get());
            }
            Activity::Finalization(f) => {
                self.state.set_view(f.proposal.round.view().get());
                self.state.inc_finalized();
            }
            Activity::Nullification(_) => {
                self.state.inc_nullified();
            }
            _ => {}
        }
        async {}
    }
}
