# Kora Node Implementation Plan

This document outlines the implementation phases for building a production-ready kora node that integrates commonware consensus with REVM execution.

## Current Status

### Implemented
- **Storage Layer**: `kora-qmdb` (pure store logic), `kora-handlers` (thread-safe wrappers), `kora-traits` (StateDb abstractions)
- **Execution Layer**: `kora-executor` with full REVM integration, all Ethereum tx types supported
- **Consensus Abstractions**: Traits for `Mempool`, `SnapshotStore`, `SeedTracker`, `BlockExecutor`
- **Default Components**: In-memory implementations of mempool, snapshot store, seed tracker

### Missing
- Consensus application integration
- Block production/proposal logic
- Block dissemination (P2P)
- Finalization and persistence
- Networking and RPC

---

## Phase 1: Core Consensus Integration

**Goal**: Create the core abstractions that bridge consensus with execution.

### 1.1 LedgerView Aggregate

Create a `LedgerView` type that owns all state management components:

```rust
pub struct LedgerView<S, M, SS, ST> {
    /// Pending transaction pool
    mempool: M,
    /// Execution snapshots keyed by digest
    snapshots: SS,
    /// VRF seeds for prevrandao
    seeds: ST,
    /// Persistent state database
    state: S,
}
```

**Location**: `crates/node/consensus/src/ledger.rs`

**Responsibilities**:
- Coordinate state access across components
- Build transaction batches for proposals (excluding in-flight)
- Manage snapshot lifecycle (insert, persist, prune)
- Track seeds for prevrandao computation

### 1.2 Application Trait

Define the consensus application interface:

```rust
pub trait ConsensusApplication: Clone + Send + Sync + 'static {
    type Block: Clone + Send + Sync;
    type Digest: Clone + Send + Sync;

    /// Propose a new block during leadership.
    fn propose(&self, parent: Self::Digest) -> Result<Self::Block, ConsensusError>;

    /// Verify a block proposed by another validator.
    fn verify(&self, block: &Self::Block) -> Result<Self::Digest, ConsensusError>;

    /// Handle block finalization.
    fn finalize(&self, digest: Self::Digest) -> Result<(), ConsensusError>;
}
```

**Location**: `crates/node/consensus/src/application.rs`

### 1.3 Proposal Builder

Logic for building block proposals:

```rust
pub struct ProposalBuilder<S, M, E> {
    state: S,
    mempool: M,
    executor: E,
    max_txs: usize,
}
```

**Responsibilities**:
- Gather transactions from mempool
- Exclude transactions in pending ancestor blocks
- Execute batch to compute state root
- Build complete block with header

---

## Phase 2: Block Lifecycle

**Goal**: Implement the full block lifecycle from proposal to finalization.

### 2.1 Block Proposal Logic
- Walk ancestry to collect pending transaction IDs
- Build batch from mempool excluding in-flight
- Execute against parent snapshot
- Compute state root from execution outcome
- Construct block header with all fields

### 2.2 Block Verification
- Receive block from peer
- Lookup parent snapshot (or rebuild from persisted state)
- Re-execute transactions
- Compare computed state root with header
- Cache snapshot on success, reject on mismatch

### 2.3 Finalization Reporter
- Observe finalized blocks from consensus
- Walk ancestry to find unpersisted ancestors
- Merge all ancestor changes in order
- Commit aggregated changes to state database
- Prune mempool of included transactions
- Mark snapshots as persisted

### 2.4 Seed Reporter
- Observe seed signatures from consensus (notarization/finalization)
- Hash seed with keccak256
- Store hash keyed by block digest
- Provide seed for next block's prevrandao

---

## Phase 3: Networking

**Goal**: Enable communication between nodes.

### 3.1 Block Dissemination (Marshal)
- Broadcast full blocks to peers during proposal
- Serve backfill requests for missing ancestors
- Integrate with commonware-broadcast

### 3.2 P2P Integration
- Use commonware-p2p for authenticated connections
- Multiple channels: votes, certificates, blocks, backfill
- Per-peer bandwidth quotas

### 3.3 RPC Endpoints
- HTTP/JSON-RPC server for transaction submission
- eth_sendRawTransaction for tx ingestion
- eth_getBalance, eth_call for state queries
- Event subscriptions (logs, blocks)

### 3.4 Transaction Relay
- Validate transactions before mempool insertion
- Relay valid transactions to peers
- Handle duplicate detection

---

## Phase 4: Production Hardening

**Goal**: Make the node production-ready.

### 4.1 Real Storage Backend
- Implement `QmdbGettable`/`QmdbBatchable` with RocksDB
- Persistent state across restarts
- Crash consistency guarantees

### 4.2 Block Hash History
- Track recent block hashes (256 blocks)
- Implement proper `block_hash_ref()` for BLOCKHASH opcode

### 4.3 Header Validation
- Gas limit bounds checking
- Timestamp ordering (monotonic)
- Parent hash verification
- Difficulty/mixHash for PoW compatibility (if needed)

### 4.4 Transaction Pre-Validation
- Nonce checking before mempool insertion
- Balance verification for gas payment
- Signature validation upfront
- Size limits and gas price minimums

### 4.5 State Recovery
- Load persisted state on startup
- Replay uncommitted blocks if needed
- Handle unclean shutdown recovery

### 4.6 Metrics and Monitoring
- Prometheus metrics for execution, consensus, networking
- Structured logging with tracing
- Health check endpoints

---

## Phase 5: Node Builder

**Goal**: Create a configurable node builder from TOML configuration.

### 5.1 Configuration Schema
```toml
[node]
chain_id = 1
data_dir = "/var/lib/kora"

[consensus]
validator_key = "path/to/key"
threshold = 2
participants = ["pk1", "pk2", "pk3"]

[network]
listen_addr = "0.0.0.0:30303"
bootstrap_peers = ["peer1:30303", "peer2:30303"]

[execution]
gas_limit = 30000000
block_time = 2

[rpc]
http_addr = "0.0.0.0:8545"
ws_addr = "0.0.0.0:8546"
```

### 5.2 NodeBuilder Type
```rust
pub struct NodeBuilder {
    config: NodeConfig,
}

impl NodeBuilder {
    pub fn from_toml(path: &Path) -> Result<Self, Error>;
    pub fn with_chain_id(self, chain_id: u64) -> Self;
    pub fn with_data_dir(self, path: PathBuf) -> Self;
    pub fn build(self) -> Result<Node, Error>;
}
```

### 5.3 Node Type
```rust
pub struct Node {
    consensus: ConsensusEngine,
    application: KoraApplication,
    network: NetworkService,
    rpc: RpcServer,
}

impl Node {
    pub async fn run(self) -> Result<(), Error>;
    pub fn shutdown(&self);
}
```

---

## Dependency Flow

```
bin/kora
    └── NodeBuilder (Phase 5)
            ├── kora-consensus
            │       ├── LedgerView (Phase 1)
            │       │       ├── Mempool
            │       │       ├── SnapshotStore
            │       │       ├── SeedTracker
            │       │       └── StateDb
            │       ├── Application (Phase 1)
            │       ├── ProposalBuilder (Phase 1)
            │       └── Reporters (Phase 2)
            ├── kora-executor
            │       ├── BlockExecutor
            │       └── RevmExecutor
            ├── kora-handlers
            │       └── QmdbHandle
            └── kora-network (Phase 3)
                    ├── Marshal
                    └── RpcServer
```

---

## Testing Strategy

Each phase should include:
- Unit tests for individual components
- Integration tests for component interactions
- Property-based tests for invariants (where applicable)

Key invariants to test:
1. Block accepted iff re-execution yields advertised state_root
2. Changes merged in digest order before persistence
3. Persistence is idempotent
4. Mempool entries removed only after finalization
5. Seeds properly propagate to prevrandao

---

## Timeline

| Phase | Components | Dependencies |
|-------|------------|--------------|
| 1 | LedgerView, Application, ProposalBuilder | Existing crates |
| 2 | Proposal/Verify logic, Reporters | Phase 1 |
| 3 | Marshal, P2P, RPC | Phase 2 |
| 4 | Storage, Validation, Recovery | Phase 3 |
| 5 | NodeBuilder, Configuration | Phase 4 |
