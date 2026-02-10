# CLAUDE.md - Kora Development Guide

## What is Kora?

Kora is a minimal, high-performance EVM-compatible execution client built entirely in Rust. It connects [REVM](https://github.com/bluealloy/revm) (Rust EVM) directly to [Commonware](https://github.com/commonwarexyz/monorepo) simplex consensus with [QMDB](https://github.com/commonwarexyz/monorepo/tree/main/storage) for state storage. It uses BLS12-381 threshold signatures for consensus and Ed25519 for node identity.

**Status:** Pre-alpha. Rust 1.88+, Edition 2024, MIT licensed.

## Build & Test Commands

```sh
cargo build --release                              # Release build
cargo build --all-targets                          # All targets
cargo nextest run --workspace --all-features       # Run tests
just ci                                            # Full CI: fmt, clippy, test, deny
cargo +nightly fmt --all -- --check                # Check formatting
cargo +nightly fmt --all                           # Fix formatting
cargo clippy --all-targets --all-features -- -D warnings  # Lint
just deny                                          # Dependency audit
```

### Development Commands

```sh
just devnet           # Start devnet with interactive DKG (production-like)
just trusted-devnet   # Start devnet with trusted dealer DKG (fast, for local dev)
just devnet-down      # Stop devnet
just devnet-logs      # View devnet logs
just devnet-status    # View devnet status
just loadtest         # Quick load test (1000 txs)
just stresstest       # Stress test (10000 txs, 50 accounts)
```

## Project Structure

```
bin/
  kora/              Main validator binary (DKG ceremony, validator, legacy modes)
  keygen/            Key generation tool (identity keys, trusted dealer DKG)
  loadgen/           Transaction load generator for benchmarking

crates/
  node/              Core node components
    builder/         Node construction with type-state pattern
    config/          Configuration types (NodeConfig, ConsensusConfig, NetworkConfig, etc.)
    consensus/       Consensus application layer (bridges Commonware <-> REVM)
    dkg/             Interactive Distributed Key Generation ceremony
    domain/          Core types: Block, Tx, BlockId, StateRoot, BootstrapConfig
    executor/        REVM-based EVM execution engine
    ledger/          State management, snapshots, mempool integration
    reporters/       Consensus event handlers (SeedReporter, FinalizedReporter, NodeStateReporter)
    rpc/             JSON-RPC server (eth_*, net_*, web3_*, kora_*)
    runner/          Production node orchestration (ProductionRunner, RevmApplication)
    service/         Node service lifecycle (KoraNodeService, LegacyNodeService)
    simplex/         Commonware simplex engine configuration and defaults
    txpool/          Transaction pool with nonce-ordering and gas price sorting

  network/           Networking layer
    marshal/         Block dissemination via commonware Marshal protocol
    transport/       P2P transport wrapping commonware authenticated discovery
    transport-sim/   Simulated transport for testing

  storage/           State storage layer
    backend/         QMDB backend (AccountStore, StorageStore, CodeStore)
    handlers/        Thread-safe QMDB handles with REVM DatabaseRef adapter
    indexer/         In-memory block/tx/receipt/log indexer for RPC queries
    overlay/         OverlayState: in-memory state overlay on QMDB base
    qmdb/            Core QMDB abstractions (QmdbStore, ChangeSet, StoreBatches)
    qmdb-ledger/     Ledger integration with QMDB (QmdbLedger, QmdbState)
    traits/          Storage trait definitions (StateDb, StateDbRead, StateDbWrite)

  utilities/         Shared utilities
    cli/             CLI helpers (backtrace, SIGSEGV handling)
    crypto/          Cryptographic utilities
    sys/             System utilities (file limits)

  e2e/               End-to-end testing framework (TestHarness, TestNode, TestSetup)

docker/              Docker compose for devnet with Prometheus/Grafana monitoring
```

## Architecture Overview

### Three-Phase Devnet Startup

1. **Phase 0 - Identity Generation:** Generate Ed25519 identity keys for each validator
2. **Phase 1 - DKG Ceremony:** Interactive threshold key generation using Ed25519 simplex consensus. Validators collaborate to generate a shared BLS12-381 threshold key
3. **Phase 2 - Consensus & Execution:** Full validator nodes running BLS12-381 threshold consensus, REVM execution, and QMDB state storage

### Node Architecture (ProductionRunner)

The `ProductionRunner` in `crates/node/runner/src/runner.rs` orchestrates all components:

```
                    Simplex Consensus Engine
                    (BLS12-381 threshold signing)
                           |
              +-----------+-----------+
              |                       |
     RevmApplication           Marshal Actor
     (propose/verify)         (block dissemination)
              |                       |
         LedgerService          FinalizedReporter
         (state + mempool)     (persist finalized blocks)
              |                       |
         RevmExecutor           OverlayState
         (EVM execution)       (pending state)
              |                       |
         StateDbAdapter            QmdbLedger
         (async->sync bridge)   (QMDB persistence)
```

### Key Type Aliases

```rust
type Peer = ed25519::PublicKey;                                    // Node identity
type ThresholdScheme = bls12381_threshold::Scheme<PublicKey, MinSig>;  // Signing
type ConsensusDigest = sha256::Digest;                             // Block commitments (32 bytes)
```

### Block Type

```rust
struct Block {
    parent: BlockId,      // Hash of parent block
    height: u64,          // Block number
    prevrandao: B256,     // VRF-derived randomness from threshold signatures
    state_root: StateRoot,// Post-execution QMDB state root
    txs: Vec<Tx>,         // Signed EVM transactions (raw bytes)
}
```

## Transaction Lifecycle

```
1. SUBMISSION (JSON-RPC)
   eth_sendRawTransaction(raw_bytes)
   -> Decode TxEnvelope (Legacy/EIP-2930/EIP-1559/EIP-4844/EIP-7702)
   -> Recover signer (ECDSA via k256)
   -> Validate (chain_id, gas, nonce, balance)
   -> Insert into TransactionPool

2. BLOCK PROPOSAL (when elected leader)
   RevmApplication::propose()
   -> Fetch pending txs from mempool (excluding already-pending in ancestors)
   -> Execute via RevmExecutor against parent OverlayState
   -> Extract ChangeSet from REVM state
   -> Compute state root via QMDB
   -> Cache snapshot in LedgerService
   -> Return Block

3. BLOCK VERIFICATION (other validators)
   RevmApplication::verify()
   -> Collect unverified blocks from ancestry stream (tip -> parent)
   -> Re-execute each block's transactions
   -> Verify computed state_root == block.state_root
   -> Cache snapshots for verified blocks

4. FINALIZATION (consensus agreement)
   FinalizedReporter::report()
   -> Re-execute if snapshot missing
   -> Persist state changes to QMDB
   -> Prune finalized txs from mempool
   -> Acknowledge to marshal (unblocks delivery floor)
```

## Commonware Framework (v0.0.65)

Kora depends on 12 commonware crates. Understanding these is essential for working on Kora.

### commonware-consensus (Simplex Protocol)

The core consensus engine. Simplex is a BFT protocol achieving:
- **2 network hops** for block times
- **3 network hops** for finalization
- Lazy message verification (only verify when 2f+1 quorum reached)
- Embedded VRF via BLS12-381 threshold signatures

**Key traits Kora implements:**
- `Application` - Proposes blocks (`propose()`) and provides genesis
- `VerifyingApplication` - Verifies blocks from other validators (`verify()`)
- `Reporter` - Handles consensus activity events (notarizations, finalizations, nullifications)

**Consensus messages:** `notarize(c,v)`, `nullify(v)`, `finalize(c,v)` - certificates require 2f+1 votes.

**Marshal subsystem:** Handles block dissemination separately from consensus votes. Provides ordered delivery of finalized blocks with acknowledgement-based flow control.

### commonware-cryptography

**Signing schemes:**
- `ed25519` - Node identity and authentication (32-byte keys, 64-byte signatures)
- `bls12381::primitives::variant::MinSig` - Threshold signatures for consensus
- `bls12381::dkg` - Joint-Feldman DKG protocol (Dealer, Player, PlayerAck)

**Key traits:** `Signer`, `PublicKey`, `Committable` (block hashing), `Digestible`, `Hasher` (SHA-256)

**Certificate system:** `Attestation` pairs (signer_index, signature). Certificates assembled from 2f+1 attestations with lazy signature deserialization.

### commonware-p2p

Authenticated, encrypted P2P networking. Kora uses `authenticated::discovery` mode with bootstrappers for peer discovery.

**Key traits:** `Sender`, `Receiver`, `Manager` (peer tracking), `Blocker` (ban misbehaving peers)

**Transport channels used by Kora:**
- Simplex: votes, certificates, resolver
- Marshal: blocks, backfill
- DKG: ceremony messages (channel 10, namespace `_KORA_DKG_CEREMONY`)

### commonware-storage (QMDB)

Quick Merkle Database - authenticated append-only storage with merkle proofs. Operations logged in append-only journal with snapshot index for fast lookups.

**State transitions:** Clean -> Mutable -> Merkleized -> Clean

### commonware-runtime

Async runtime abstraction. Kora uses the `tokio` implementation.

**Key traits:** `Spawner`, `Clock`, `Metrics`, `Storage`

**Buffer pools:** `PoolRef` with configurable page size (64 KiB default) and capacity (10,000 pages).

### Other commonware crates

- `commonware-codec` - Binary serialization with safety constraints (size limits, configs)
- `commonware-broadcast` - Buffered broadcast for block dissemination
- `commonware-resolver` - Fetches missing data from peers (used by Simplex for catch-up)
- `commonware-parallel` - Execution strategy (`Sequential` used by Kora)
- `commonware-stream` - Low-level encrypted message exchange
- `commonware-macros` - Procedural macros (`select!`, stability annotations)
- `commonware-utils` - `NZU64`/`NZUsize` wrappers, `Faults`/`N3f1` BFT utilities, `Participant`

## EVM Execution (REVM)

### RevmExecutor (`crates/node/executor/src/revm.rs`)

Executes EVM transactions against a `StateDb`. Supports all Ethereum transaction types:
- Legacy, EIP-2930 (access lists), EIP-1559 (dynamic fees), EIP-4844 (blobs), EIP-7702 (delegation)

**Default hardfork:** Cancun (`SpecId::CANCUN`)

**Execution flow:**
1. Wrap `StateDb` in `StateDbAdapter` (async->sync bridge using `block_on`)
2. Build REVM `State` database from adapter
3. Configure block environment (number, timestamp, gas_limit, beneficiary, prevrandao)
4. For each transaction: decode `TxEnvelope`, set `TxEnv`, call `evm.replay()`
5. Build `ExecutionReceipt` per tx, extract `ChangeSet` from REVM `EvmState`

**Gas configuration:** Default 30M gas limit, EIP-1559 base fee params, gas limit bounds with max delta divisor.

### State Management

**Two-layer storage architecture:**
1. **kora-qmdb** (Layer 1): Pure store logic with `QmdbStore` owning three partitions (accounts, storage, code). `ChangeSet` accumulates state changes with merge capability
2. **kora-handlers** (Layer 2): Thread-safe `QmdbHandle` with `Arc<RwLock>`, implements REVM `DatabaseRef`

**OverlayState pattern:** Layers in-memory `ChangeSet` on top of a base `QmdbState`. Used during block execution to provide pending ancestor changes without persisting. Multiple overlays can stack for speculative execution.

**StateDb trait hierarchy:**
```rust
StateDbRead  -> nonce, balance, code_hash, code, storage (all async)
StateDbWrite -> commit(ChangeSet) -> B256, compute_root(&ChangeSet) -> B256, merge_changes
StateDb      -> StateDbRead + StateDbWrite + state_root()
```

## DKG (Distributed Key Generation)

### Interactive DKG (`crates/node/dkg/`)

Uses commonware's Joint-Feldman DKG over BLS12-381.

**Protocol phases:**
1. **Dealer Start:** Generate commitment polynomials and shares, broadcast public commitments, send private shares
2. **Collect & Ack:** Receive commitments, verify private shares, send acknowledgements
3. **Synchronization (Phase 2.5):** Broadcast "ready" signal, wait for all participants
4. **Finalize Dealer:** Create signed dealer log with collected acks
5. **Collect Logs:** Gather dealer logs from quorum participants
6. **Finalization:** Compute BLS12-381 shares, generate group public key

**Security:** Anti-replay via ceremony ID (hash of chain_id + sorted keys + timestamp), SHA256-based deduplication, sender verification.

**Output persisted to:** `{data_dir}/output.json` (public), `{data_dir}/share.key` (secret)

### Threshold Scheme Loading (`crates/node/runner/src/scheme.rs`)

```rust
type ThresholdScheme = bls12381_threshold::Scheme<ed25519::PublicKey, MinSig>;
// Loaded with namespace: _COMMONWARE_KORA_SIMPLEX
```

## Consensus Integration

### Reporters (`crates/node/reporters/`)

Three reporters compose into the consensus pipeline:

1. **SeedReporter** - Extracts VRF seeds from notarizations/finalizations, stores keccak256(seed) in LedgerService for future block `prevrandao` fields
2. **FinalizedReporter** - Handles finalized blocks: re-executes if snapshot missing, persists to QMDB, prunes mempool, acknowledges to marshal
3. **NodeStateReporter** - Updates RPC-visible state (view number, finalized count, nullified count)

### Consensus Parameters (from ProductionRunner)

```rust
leader_timeout:       500ms    // Wait for leader proposal
notarization_timeout: 1s       // Wait for 2f+1 notarize votes
nullify_retry:        2s       // Retry nullification
fetch_timeout:        500ms    // Fetch missing blocks
activity_timeout:     20 views // Inactivity threshold
skip_timeout:         10 views // Skip threshold
max_txs_per_block:    64
max_tx_bytes:         1024
epoch_length:         u64::MAX // Single epoch per node lifetime
mailbox_size:         4096
```

## RPC Server (`crates/node/rpc/`)

Implements standard Ethereum JSON-RPC:

**eth_ namespace:** chainId, blockNumber, getBalance, getTransactionCount, getCode, getStorageAt, sendRawTransaction, call, estimateGas, getBlockByNumber/Hash, getTransactionByHash, getTransactionReceipt, gasPrice, maxPriorityFeePerGas, feeHistory, getLogs

**net_ namespace:** version, listening, peerCount

**web3_ namespace:** clientVersion, sha3

**kora_ namespace:** Custom Kora-specific endpoints

**State providers:** `StateProvider` trait abstracts state access. `IndexedStateProvider` provides full queries. `NoopStateProvider` returns defaults.

RPC port per validator: `8545 + share_index`

## Code Conventions

- Use `#![doc = include_str!("../README.md")]` in lib.rs to embed README as docs
- Every crate has a README.md with badges
- Use workspace lints and dependencies (defined in root Cargo.toml)
- Follow type-state pattern for builders
- Use `thiserror` for error types, `eyre` at binary level, `anyhow` in runner
- Prefer `const fn` where possible
- `#[must_use]` on builder methods
- Non-exhaustive structs for future extensibility

### Workspace Lints

- `missing-docs` (warn)
- `missing-debug-implementations` (warn)
- `unreachable-pub` (warn)
- `unused-must-use` (deny)
- `rust-2018-idioms` (deny)
- Clippy: all warn, `missing-const-for-fn`, `use-self`, `option-if-let-else`, `redundant-clone`

### Testing

- Unit tests use `#[cfg(test)]` modules in each crate
- E2E tests in `crates/e2e` with `TestHarness`, `TestNode`, `TestSetup`
- Use `rstest` for parameterized tests
- Use `tempfile` for temporary storage
- Commonware's `tokio::Runner` for async test execution
- Test runner: `cargo nextest run`

## Key Dependencies

```toml
# Commonware (all at 0.0.65)
commonware-consensus, commonware-cryptography, commonware-p2p,
commonware-storage, commonware-runtime, commonware-codec,
commonware-broadcast, commonware-resolver, commonware-parallel,
commonware-stream, commonware-macros, commonware-utils

# EVM
revm = "34.0.0"          # EVM execution
alloy-primitives = "1.0" # Address, B256, U256, keccak256
alloy-consensus = "1.0"  # TxEnvelope, Header, transaction types
alloy-eips = "1.0"       # EIP-2930, EIP-4844, EIP-7702
alloy-rlp = "0.3"        # RLP encoding/decoding
alloy-evm = "0.27.0"     # EVM utilities

# Async: tokio 1, futures 0.3
# Serialization: serde 1, serde_json 1, toml 0.8
# CLI: clap 4
# Crypto: k256 0.13, sha3 0.10
# Errors: thiserror 2, eyre 0.6, anyhow 1
# Metrics: prometheus-client 0.24
```

## Configuration

### NodeConfig hierarchy

```
NodeConfig
  chain_id: u64             (default: 1)
  data_dir: PathBuf         (default: /var/lib/kora)
  consensus: ConsensusConfig
    validator_key: Option<PathBuf>
    threshold: u32          (default: 2)
  network: NetworkConfig
    listen_addr: String     (default: 0.0.0.0:30303)
    bootstrap_peers: Vec<String>
  execution: ExecutionConfig
    gas_limit: u64          (default: 30_000_000)
    block_time: u64         (default: 2)
  rpc: RpcConfig
    http_addr: String       (default: 0.0.0.0:8545)
    ws_addr: String         (default: 0.0.0.0:8546)
```

### CLI

```
kora                          # Legacy mode (no DKG)
kora dkg --peers peers.json   # Run interactive DKG ceremony
kora validator --peers peers.json  # Run validator with threshold consensus
```

Global flags: `--config FILE`, `--chain-id N`, `--data-dir PATH`, `--verbose`

## Build Profiles

```toml
[profile.dev]      # Line tables only, packed debuginfo, no incremental
[profile.release]  # opt-level 3, thin LTO, no debug, stripped symbols
[profile.maxperf]  # Fat LTO, codegen-units 1 (maximum performance)
```
