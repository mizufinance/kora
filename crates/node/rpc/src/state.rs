//! Node state management for RPC endpoints.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use parking_lot::RwLock;
use serde::Serialize;

/// Shared node state that can be updated by the consensus engine.
#[derive(Debug, Clone)]
pub struct NodeState {
    inner: Arc<NodeStateInner>,
}

#[derive(Debug)]
struct NodeStateInner {
    chain_id: u64,
    validator_index: u32,
    started_at: Instant,
    current_view: AtomicU64,
    finalized_count: AtomicU64,
    proposed_count: AtomicU64,
    nullified_count: AtomicU64,
    peer_count: AtomicU64,
    is_leader: RwLock<bool>,
}

impl NodeState {
    /// Create a new node state.
    pub fn new(chain_id: u64, validator_index: u32) -> Self {
        Self {
            inner: Arc::new(NodeStateInner {
                chain_id,
                validator_index,
                started_at: Instant::now(),
                current_view: AtomicU64::new(0),
                finalized_count: AtomicU64::new(0),
                proposed_count: AtomicU64::new(0),
                nullified_count: AtomicU64::new(0),
                peer_count: AtomicU64::new(0),
                is_leader: RwLock::new(false),
            }),
        }
    }

    /// Update the current view.
    pub fn set_view(&self, view: u64) {
        self.inner.current_view.store(view, Ordering::Relaxed);
        // Compute leader: view mod 4 (for 4 validators)
        let is_leader = (view % 4) as u32 == self.inner.validator_index;
        *self.inner.is_leader.write() = is_leader;
    }

    /// Increment finalized block count.
    pub fn inc_finalized(&self) {
        self.inner.finalized_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment proposed block count.
    pub fn inc_proposed(&self) {
        self.inner.proposed_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment nullified round count.
    pub fn inc_nullified(&self) {
        self.inner.nullified_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Update peer count.
    pub fn set_peer_count(&self, count: u64) {
        self.inner.peer_count.store(count, Ordering::Relaxed);
    }

    /// Get current node status.
    pub fn status(&self) -> NodeStatus {
        NodeStatus {
            chain_id: self.inner.chain_id,
            validator_index: self.inner.validator_index,
            uptime_secs: self.inner.started_at.elapsed().as_secs(),
            current_view: self.inner.current_view.load(Ordering::Relaxed),
            finalized_count: self.inner.finalized_count.load(Ordering::Relaxed),
            proposed_count: self.inner.proposed_count.load(Ordering::Relaxed),
            nullified_count: self.inner.nullified_count.load(Ordering::Relaxed),
            peer_count: self.inner.peer_count.load(Ordering::Relaxed),
            is_leader: *self.inner.is_leader.read(),
        }
    }
}

/// Serializable node status for RPC responses.
#[derive(Debug, Clone, Serialize)]
pub struct NodeStatus {
    /// Chain ID.
    pub chain_id: u64,
    /// This validator's index (0-3).
    pub validator_index: u32,
    /// Seconds since node started.
    pub uptime_secs: u64,
    /// Current consensus view number.
    pub current_view: u64,
    /// Number of finalized blocks.
    pub finalized_count: u64,
    /// Number of blocks proposed by this node.
    pub proposed_count: u64,
    /// Number of nullified rounds.
    pub nullified_count: u64,
    /// Number of connected peers.
    pub peer_count: u64,
    /// Whether this node is the current leader.
    pub is_leader: bool,
}
