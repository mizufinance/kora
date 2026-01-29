//! REVM-based consensus application implementation.

use std::collections::BTreeSet;

use alloy_consensus::Header;
use alloy_primitives::{Address, B256, Bytes};
use commonware_consensus::{
    Block as _,
    marshal::ingress::mailbox::AncestorStream, simplex::types::Context, Application,
    VerifyingApplication,
};
use commonware_cryptography::{Committable as _, certificate::Scheme as CertScheme};
use commonware_runtime::{Clock, Metrics, Spawner};
use futures::StreamExt;
use kora_consensus::{BlockExecution, SnapshotStore, components::InMemorySnapshotStore};
use kora_domain::{Block, ConsensusDigest, Tx};
use kora_executor::{BlockContext, BlockExecutor};
use kora_ledger::LedgerService;
use kora_overlay::OverlayState;
use kora_qmdb_ledger::QmdbState;
use rand::Rng;
use tracing::{debug, trace, warn};

#[derive(Clone)]
pub struct RevmApplication<S, E> {
    ledger: LedgerService,
    executor: E,
    max_txs: usize,
    gas_limit: u64,
    _scheme: std::marker::PhantomData<S>,
}

impl<S, E> std::fmt::Debug for RevmApplication<S, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RevmApplication")
            .field("max_txs", &self.max_txs)
            .field("gas_limit", &self.gas_limit)
            .finish_non_exhaustive()
    }
}

impl<S, E> RevmApplication<S, E>
where
    E: BlockExecutor<OverlayState<QmdbState>, Tx = Bytes> + Clone,
{
    pub fn new(ledger: LedgerService, executor: E, max_txs: usize, gas_limit: u64) -> Self {
        Self {
            ledger,
            executor,
            max_txs,
            gas_limit,
            _scheme: std::marker::PhantomData,
        }
    }

    fn block_context(&self, height: u64, prevrandao: B256) -> BlockContext {
        let header = Header {
            number: height,
            timestamp: height,
            gas_limit: self.gas_limit,
            beneficiary: Address::ZERO,
            base_fee_per_gas: Some(0),
            ..Default::default()
        };
        BlockContext::new(header, prevrandao)
    }

    async fn get_prevrandao(&self, parent_digest: ConsensusDigest) -> B256 {
        self.ledger.seed_for_parent(parent_digest).await.unwrap_or(B256::ZERO)
    }

    async fn build_block(&self, parent: &Block) -> Option<Block> {
        use kora_consensus::Mempool as _;
        
        let parent_digest = parent.commitment();
        let parent_snapshot = self.ledger.parent_snapshot(parent_digest).await?;
        let (_, mempool, snapshots) = self.ledger.proposal_components().await;
        let excluded = self.collect_pending_tx_ids(&snapshots, parent_digest);
        let txs = mempool.build(self.max_txs, &excluded);

        let prevrandao = self.get_prevrandao(parent_digest).await;
        let height = parent.height + 1;
        let context = self.block_context(height, prevrandao);
        let txs_bytes: Vec<Bytes> = txs.iter().map(|tx| tx.bytes.clone()).collect();

        let outcome = self
            .executor
            .execute(&parent_snapshot.state, &context, &txs_bytes)
            .ok()?;

        let state_root = self
            .ledger
            .compute_root_from_store(parent_digest, outcome.changes.clone())
            .await
            .ok()?;

        let block = Block {
            parent: parent.id(),
            height,
            prevrandao,
            state_root,
            txs,
        };

        let merged_changes = parent_snapshot.state.merge_changes(outcome.changes.clone());
        let next_state = OverlayState::new(parent_snapshot.state.base(), merged_changes);
        let block_digest = block.commitment();

        self.ledger
            .insert_snapshot(
                block_digest,
                parent_digest,
                next_state,
                state_root,
                outcome.changes,
                &block.txs,
            )
            .await;

        trace!(?block_digest, height, "built block");
        Some(block)
    }

    async fn verify_block(&self, block: &Block) -> bool {
        let digest = block.commitment();
        let parent_digest = block.parent();

        if self.ledger.query_state_root(digest).await.is_some() {
            trace!(?digest, "block already verified");
            return true;
        }

        let Some(parent_snapshot) = self.ledger.parent_snapshot(parent_digest).await else {
            warn!(?digest, ?parent_digest, "missing parent snapshot");
            return false;
        };

        let context = self.block_context(block.height, block.prevrandao);
        let execution =
            match BlockExecution::execute(&parent_snapshot, &self.executor, &context, &block.txs)
                .await
            {
                Ok(result) => result,
                Err(err) => {
                    warn!(?digest, error = ?err, "execution failed");
                    return false;
                }
            };

        let state_root = match self
            .ledger
            .compute_root_from_store(parent_digest, execution.outcome.changes.clone())
            .await
        {
            Ok(root) => root,
            Err(err) => {
                warn!(?digest, error = ?err, "compute root failed");
                return false;
            }
        };

        if state_root != block.state_root {
            warn!(
                ?digest,
                expected = ?block.state_root,
                computed = ?state_root,
                "state root mismatch"
            );
            return false;
        }

        let merged_changes = parent_snapshot.state.merge_changes(execution.outcome.changes.clone());
        let next_state = OverlayState::new(parent_snapshot.state.base(), merged_changes);

        self.ledger
            .insert_snapshot(
                digest,
                parent_digest,
                next_state,
                state_root,
                execution.outcome.changes,
                &block.txs,
            )
            .await;

        debug!(?digest, "block verified");
        true
    }

    fn collect_pending_tx_ids(
        &self,
        snapshots: &InMemorySnapshotStore<OverlayState<QmdbState>>,
        from: ConsensusDigest,
    ) -> BTreeSet<kora_consensus::TxId> {
        let mut excluded = BTreeSet::new();
        let mut current = Some(from);

        while let Some(digest) = current {
            if snapshots.is_persisted(&digest) {
                break;
            }
            let Some(snapshot) = snapshots.get(&digest) else {
                break;
            };
            excluded.extend(snapshot.tx_ids.iter().copied());
            current = snapshot.parent;
        }

        excluded
    }
}

impl<Env, S, E> Application<Env> for RevmApplication<S, E>
where
    Env: Rng + Spawner + Metrics + Clock,
    S: CertScheme + Send + Sync + 'static,
    E: BlockExecutor<OverlayState<QmdbState>, Tx = Bytes> + Clone + Send + Sync + 'static,
{
    type SigningScheme = S;
    type Context = Context<ConsensusDigest, S::PublicKey>;
    type Block = Block;

    fn genesis(&mut self) -> impl std::future::Future<Output = Self::Block> + Send {
        async move { self.ledger.genesis_block() }
    }

    fn propose(
        &mut self,
        _context: (Env, Self::Context),
        mut ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> impl std::future::Future<Output = Option<Self::Block>> + Send {
        async move {
            let parent = ancestry.next().await?;
            self.build_block(&parent).await
        }
    }
}

impl<Env, S, E> VerifyingApplication<Env> for RevmApplication<S, E>
where
    Env: Rng + Spawner + Metrics + Clock,
    S: CertScheme + Send + Sync + 'static,
    E: BlockExecutor<OverlayState<QmdbState>, Tx = Bytes> + Clone + Send + Sync + 'static,
{
    fn verify(
        &mut self,
        _context: (Env, Self::Context),
        mut ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> impl std::future::Future<Output = bool> + Send {
        async move {
            // Collect all available ancestry blocks from the stream.
            // The stream yields tip-first (newest → oldest), so we reverse
            // to verify in parent-first order, ensuring each block's parent
            // snapshot exists before we attempt to verify it.
            let mut blocks = Vec::new();
            while let Some(block) = ancestry.next().await {
                blocks.push(block);
            }

            if blocks.is_empty() {
                return false;
            }

            // Verify from oldest (parent) to newest (tip)
            for block in blocks.into_iter().rev() {
                if !self.verify_block(&block).await {
                    return false;
                }
            }

            true
        }
    }
}
