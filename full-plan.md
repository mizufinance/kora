# Kora: Minimal REVM + Commonware Node Architecture Plan

## Overview

This plan describes the crate structure for building a minimal revm + commonware node. The design follows patterns from the commonware deployer crate (logic owned by types) and PR #2495 (REVM example chain).

## Design Principles

1. **Logic owned by types** - Methods on structs, not global functions
2. **Trait abstractions** - Define traits at crate boundaries for testability
3. **Type-safe configuration** - Config types with validation methods
4. **Strongly-typed identifiers** - BlockId, TxId, StateRoot as distinct types
5. **Error enums with thiserror** - Type-safe error handling
6. **No functional patterns** - Avoid closures/lambdas for core logic

---

## Crate Structure

```
kora/
├── bin/
│   └── kora/                    # Main binary (exists)
├── crates/
│   ├── primitives/              # Core primitive types (exists, expand)
│   ├── domain/                  # Domain types and logic (NEW)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types/           # Block, Tx, BlockId, StateRoot, TxId
│   │       ├── commitment.rs    # StateChanges, AccountChange
│   │       └── events.rs        # Domain events
│   ├── storage/                 # Storage abstraction layer (NEW)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs        # Storage traits
│   │       ├── adapter.rs       # REVM DatabaseRef adapter
│   │       ├── changes.rs       # Change set tracking
│   │       └── keys.rs          # Key encoding/decoding
│   ├── execution/               # EVM execution layer (NEW)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs        # Execution traits
│   │       ├── evm.rs           # EVM environment builder
│   │       ├── precompiles.rs   # Custom precompiles
│   │       └── executor.rs      # Transaction execution
│   ├── application/             # Consensus-facing application (NEW)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs        # Application traits
│   │       ├── app.rs           # Application implementation
│   │       └── ledger/          # Ledger state management
│   │           ├── mod.rs
│   │           ├── mempool.rs
│   │           ├── snapshot.rs
│   │           └── service.rs
│   ├── node/                    # Node wiring and startup (NEW)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs        # Node configuration
│   │       ├── channels.rs      # Inter-component channels
│   │       ├── handle.rs        # Node handle for external access
│   │       └── start.rs         # Node startup orchestration
│   └── utilities/
│       └── cli/                 # CLI utilities (exists)
```

---

## Dependency Graph

```
                                    ┌─────────────────┐
                                    │   bin/kora      │
                                    └────────┬────────┘
                                             │
                    ┌────────────────────────┼────────────────────────┐
                    │                        │                        │
                    ▼                        ▼                        ▼
          ┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐
          │   kora-node     │      │   kora-cli      │      │                 │
          └────────┬────────┘      └─────────────────┘      │                 │
                   │                                         │                 │
         ┌─────────┴─────────┐                              │                 │
         │                   │                              │                 │
         ▼                   ▼                              │                 │
┌─────────────────┐ ┌─────────────────┐                    │                 │
│ kora-application│ │ kora-execution  │                    │                 │
└────────┬────────┘ └────────┬────────┘                    │                 │
         │                   │                              │                 │
         │    ┌──────────────┘                              │                 │
         │    │                                             │                 │
         ▼    ▼                                             │                 │
┌─────────────────┐                                         │                 │
│  kora-storage   │                                         │                 │
└────────┬────────┘                                         │                 │
         │                                                  │                 │
         ▼                                                  │                 │
┌─────────────────┐                                         │                 │
│   kora-domain   │◀────────────────────────────────────────┘                 │
└────────┬────────┘                                                           │
         │                                                                    │
         ▼                                                                    │
┌─────────────────┐                                                           │
│ kora-primitives │◀──────────────────────────────────────────────────────────┘
└─────────────────┘
```

---

## Crate Details

### 1. `kora-primitives` (Expand existing)

**Purpose:** Core primitive types and chain constants.

**Key Types:**
```rust
/// Chain configuration constants owned by a type.
#[derive(Debug, Clone, Copy)]
pub struct ChainConfig;

impl ChainConfig {
    pub const CHAIN_ID: u64 = 1337;
    pub const MAX_TXS_PER_BLOCK: usize = 1000;
    pub const GAS_LIMIT_TRANSFER: u64 = 21_000;
}
```

**Exports:** Re-exports from alloy-primitives (Address, B256, U256, Bytes)

---

### 2. `kora-domain` (New)

**Purpose:** Domain types representing blockchain state. All types own their serialization, validation, and ID computation logic.

**Key Types:**

```rust
// types/ids.rs - Strongly-typed identifiers
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(pub B256);

impl BlockId {
    pub const ZERO: Self = Self(B256::ZERO);
    pub fn as_bytes(&self) -> &[u8; 32] { self.0.as_ref() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateRoot(pub B256);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct TxId(pub B256);
```

```rust
// types/block.rs - Block with owned logic
#[derive(Clone, Copy, Debug)]
pub struct BlockCfg {
    pub max_txs: usize,
    pub tx: TxCfg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub parent: BlockId,
    pub height: u64,
    pub prevrandao: B256,
    pub state_root: StateRoot,
    pub txs: Vec<Tx>,
}

impl Block {
    /// Compute block identifier (logic owned by type).
    pub fn id(&self) -> BlockId {
        BlockId(keccak256(self.encode()))
    }

    /// Validate block structure.
    pub fn validate(&self, cfg: &BlockCfg) -> Result<(), BlockError> {
        if self.txs.len() > cfg.max_txs {
            return Err(BlockError::TooManyTransactions(self.txs.len()));
        }
        for (i, tx) in self.txs.iter().enumerate() {
            tx.validate(&cfg.tx).map_err(|e| BlockError::InvalidTransaction { index: i, source: e })?;
        }
        Ok(())
    }
}
```

```rust
// types/tx.rs - Transaction with owned logic
#[derive(Clone, Copy, Debug)]
pub struct TxCfg {
    pub max_data_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tx {
    pub from: Address,
    pub to: Address,
    pub value: U256,
    pub gas_limit: u64,
    pub data: Bytes,
}

impl Tx {
    /// Compute transaction identifier.
    pub fn id(&self) -> TxId {
        TxId(keccak256(self.encode()))
    }

    /// Validate transaction parameters.
    pub fn validate(&self, cfg: &TxCfg) -> Result<(), TxError> {
        if self.data.len() > cfg.max_data_len {
            return Err(TxError::DataTooLarge(self.data.len()));
        }
        if self.gas_limit == 0 {
            return Err(TxError::ZeroGasLimit);
        }
        Ok(())
    }
}
```

```rust
// commitment.rs - State change tracking
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountChange {
    pub touched: bool,
    pub created: bool,
    pub selfdestructed: bool,
    pub nonce: u64,
    pub balance: U256,
    pub code_hash: B256,
    pub storage: BTreeMap<U256, U256>,
}

impl AccountChange {
    pub fn is_deletion(&self) -> bool { self.selfdestructed }
    pub fn has_storage_changes(&self) -> bool { !self.storage.is_empty() }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateChanges {
    pub accounts: BTreeMap<Address, AccountChange>,
}

impl StateChanges {
    pub fn merge(&mut self, other: StateChanges) {
        for (addr, change) in other.accounts {
            self.accounts.insert(addr, change);
        }
    }

    pub fn is_empty(&self) -> bool { self.accounts.is_empty() }
}
```

```rust
// error.rs - Type-safe errors
#[derive(Debug, Error)]
pub enum BlockError {
    #[error("too many transactions: {0} > max")]
    TooManyTransactions(usize),
    #[error("invalid transaction at index {index}: {source}")]
    InvalidTransaction { index: usize, #[source] source: TxError },
}

#[derive(Debug, Error)]
pub enum TxError {
    #[error("transaction data too large: {0} bytes")]
    DataTooLarge(usize),
    #[error("gas limit cannot be zero")]
    ZeroGasLimit,
}
```

---

### 3. `kora-storage` (New)

**Purpose:** Storage abstraction layer bridging REVM database traits with backend storage.

**Key Traits:**

```rust
// traits.rs - Storage trait abstractions
#[async_trait]
pub trait AccountReader: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn read_account(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error>;
    async fn read_storage(&self, address: Address, slot: U256) -> Result<U256, Self::Error>;
    async fn read_code(&self, code_hash: B256) -> Result<Option<Bytes>, Self::Error>;
}

#[async_trait]
pub trait AccountWriter: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn write_changes(&mut self, changes: &StateChanges) -> Result<(), Self::Error>;
    async fn compute_root(&self, changes: &StateChanges) -> Result<StateRoot, Self::Error>;
}

/// Combined trait for full state access.
pub trait StateStore: AccountReader + AccountWriter {}
impl<T: AccountReader + AccountWriter> StateStore for T {}
```

```rust
// adapter.rs - REVM database adapter (type owns bridging logic)
#[derive(Clone)]
pub struct StorageAdapter<S> {
    store: Arc<Mutex<S>>,
}

impl<S> StorageAdapter<S> {
    pub fn new(store: S) -> Self {
        Self { store: Arc::new(Mutex::new(store)) }
    }
}

impl<S: AccountReader> DatabaseAsyncRef for StorageAdapter<S> {
    type Error = StorageError;

    async fn basic_async_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        let store = self.store.lock().await;
        store.read_account(address).await.map_err(StorageError::from)
    }
}

/// Sync wrapper using tokio handle.
#[derive(Clone)]
pub struct SyncStorageAdapter<S> {
    inner: Arc<WrapDatabaseAsync<StorageAdapter<S>>>,
}

impl<S: AccountReader> SyncStorageAdapter<S> {
    pub fn new(store: S, runtime: tokio::runtime::Handle) -> Self {
        let adapter = StorageAdapter::new(store);
        Self { inner: Arc::new(WrapDatabaseAsync::with_handle(adapter, runtime)) }
    }
}
```

```rust
// changes.rs - Change set tracking (type owns merge logic)
#[derive(Clone, Debug, Default)]
pub struct ChangeSet {
    accounts: BTreeMap<Address, AccountUpdate>,
}

#[derive(Clone, Debug)]
pub struct AccountUpdate {
    pub created: bool,
    pub selfdestructed: bool,
    pub nonce: u64,
    pub balance: U256,
    pub code_hash: B256,
    pub code: Option<Vec<u8>>,
    pub storage: BTreeMap<U256, U256>,
}

impl ChangeSet {
    pub fn apply(&mut self, address: Address, update: AccountUpdate) {
        if let Some(existing) = self.accounts.get_mut(&address) {
            existing.merge(update);
        } else {
            self.accounts.insert(address, update);
        }
    }

    pub fn merge(&mut self, other: ChangeSet) {
        for (addr, update) in other.accounts {
            self.apply(addr, update);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Address, &AccountUpdate)> {
        self.accounts.iter()
    }
}

impl AccountUpdate {
    pub fn merge(&mut self, other: AccountUpdate) {
        self.nonce = other.nonce;
        self.balance = other.balance;
        self.code_hash = other.code_hash;
        if other.code.is_some() { self.code = other.code; }
        if other.selfdestructed {
            self.selfdestructed = true;
            self.storage.clear();
        } else {
            for (slot, value) in other.storage {
                self.storage.insert(slot, value);
            }
        }
    }
}
```

---

### 4. `kora-execution` (New)

**Purpose:** EVM execution layer. All execution logic encapsulated in types.

**Key Types:**

```rust
// evm.rs - EVM environment configuration (type owns building logic)
#[derive(Clone, Debug)]
pub struct EvmConfig {
    pub chain_id: u64,
    pub spec_id: SpecId,
}

impl EvmConfig {
    pub const fn kora() -> Self {
        Self { chain_id: ChainConfig::CHAIN_ID, spec_id: SpecId::CANCUN }
    }
}

#[derive(Clone)]
pub struct EvmEnvBuilder {
    config: EvmConfig,
}

impl EvmEnvBuilder {
    pub fn new(config: EvmConfig) -> Self { Self { config } }

    pub fn build_for_block(&self, height: u64, prevrandao: B256) -> EvmEnv {
        let mut env = EvmEnv::default();
        env.cfg_env.chain_id = self.config.chain_id;
        env.cfg_env.spec = self.config.spec_id;
        env.block_env.number = U256::from(height);
        env.block_env.timestamp = U256::from(height);
        env.block_env.prevrandao = Some(prevrandao);
        env
    }
}
```

```rust
// precompiles.rs - Custom precompiles (type owns installation logic)
pub const SEED_PRECOMPILE_ADDRESS: [u8; 20] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0xFF,
];

#[derive(Clone)]
pub struct KoraPrecompiles {
    inner: PrecompilesMap,
}

impl KoraPrecompiles {
    pub fn new(spec: SpecId) -> Self {
        let mut precompiles = PrecompilesMap::from_static(
            Precompiles::new(PrecompileSpecId::from_spec_id(spec))
        );
        Self::install_seed_precompile(&mut precompiles);
        Self { inner: precompiles }
    }

    fn install_seed_precompile(precompiles: &mut PrecompilesMap) {
        let address = Address::from(SEED_PRECOMPILE_ADDRESS);
        precompiles.apply_precompile(&address, |_| {
            Some(DynPrecompile::new_stateful(
                PrecompileId::Custom("kora_seed".into()),
                |input| {
                    let seed = input.internals.block_env().prevrandao().unwrap_or(B256::ZERO);
                    Ok(PrecompileOutput::new(0, Bytes::copy_from_slice(seed.as_slice())))
                },
            ))
        });
    }

    pub fn into_map(self) -> PrecompilesMap { self.inner }
}
```

```rust
// traits.rs - Executor trait abstraction for testability
pub trait Executor: Clone + Send + Sync {
    fn execute<DB>(
        &self,
        db: DB,
        height: u64,
        prevrandao: B256,
        txs: &[Tx],
    ) -> Result<ExecutionOutcome<DB>, ExecutionError>
    where
        DB: Database + DatabaseCommit;
}
```

```rust
// executor.rs - Transaction executor (type owns execution logic)
#[derive(Debug, Clone)]
pub struct ExecutionOutcome<DB> {
    pub db: DB,
    pub tx_changes: Vec<StateChanges>,
    pub storage_changes: ChangeSet,
}

#[derive(Clone)]
pub struct DefaultExecutor {
    env_builder: EvmEnvBuilder,
    precompiles: KoraPrecompiles,
}

impl Executor for DefaultExecutor {
    fn execute<DB>(
        &self,
        db: DB,
        height: u64,
        prevrandao: B256,
        txs: &[Tx],
    ) -> Result<ExecutionOutcome<DB>, ExecutionError>
    where
        DB: Database + DatabaseCommit,
    {
        self.execute_impl(db, height, prevrandao, txs)
    }
}

impl DefaultExecutor {
    pub fn new(config: EvmConfig) -> Self {
        Self {
            precompiles: KoraPrecompiles::new(config.spec_id),
            env_builder: EvmEnvBuilder::new(config),
        }
    }

    pub fn execute<DB>(
        &self,
        db: DB,
        height: u64,
        prevrandao: B256,
        txs: &[Tx],
    ) -> Result<ExecutionOutcome<DB>, ExecutionError>
    where
        DB: Database + DatabaseCommit,
    {
        let env = self.env_builder.build_for_block(height, prevrandao);
        let mut evm = EthEvmBuilder::new(db, env)
            .precompiles(self.precompiles.clone().into_map())
            .build();

        let chain_id = evm.chain_id();
        let mut tx_changes = Vec::with_capacity(txs.len());
        let mut storage_changes = ChangeSet::default();

        for tx in txs {
            let tx_env = self.build_tx_env(evm.db_mut(), tx, chain_id)?;
            let ResultAndState { result, state } = evm.transact_raw(tx_env)
                .map_err(ExecutionError::EvmError)?;

            if !result.is_success() {
                return Err(ExecutionError::TxFailed(result));
            }

            let changes = self.extract_state_changes(&state);
            self.apply_to_storage_changes(&mut storage_changes, &state);
            evm.db_mut().commit(state);
            tx_changes.push(changes);
        }

        let (db, _) = evm.finish();
        Ok(ExecutionOutcome { db, tx_changes, storage_changes })
    }

    fn build_tx_env<DB: Database>(&self, db: &mut DB, tx: &Tx, chain_id: u64) -> Result<TxEnv, ExecutionError> {
        let nonce = match db.basic(tx.from).map_err(|_| ExecutionError::DatabaseRead)? {
            Some(info) => info.nonce,
            None => 0,
        };
        Ok(TxEnv {
            caller: tx.from,
            kind: TxKind::Call(tx.to),
            value: tx.value,
            gas_limit: tx.gas_limit,
            data: tx.data.clone(),
            nonce,
            chain_id: Some(chain_id),
            gas_price: 0,
            ..Default::default()
        })
    }

    fn extract_state_changes(&self, state: &EvmState) -> StateChanges {
        let mut changes = StateChanges::default();
        for (address, account) in state.iter() {
            if !account.is_touched() { continue; }
            changes.accounts.insert(*address, AccountChange {
                touched: account.is_touched(),
                created: account.is_created(),
                selfdestructed: account.is_selfdestructed(),
                nonce: account.info.nonce,
                balance: account.info.balance,
                code_hash: account.info.code_hash,
                storage: account.changed_storage_slots()
                    .map(|(k, v)| (*k, v.present_value()))
                    .collect(),
            });
        }
        changes
    }

    fn apply_to_storage_changes(&self, changes: &mut ChangeSet, state: &EvmState) {
        for (address, account) in state.iter() {
            if !account.is_touched() { continue; }
            let update = AccountUpdate {
                created: account.is_created(),
                selfdestructed: account.is_selfdestructed(),
                nonce: account.info.nonce,
                balance: account.info.balance,
                code_hash: account.info.code_hash,
                code: account.info.code.as_ref().map(|c| c.original_byte_slice().to_vec()),
                storage: account.changed_storage_slots()
                    .map(|(k, v)| (*k, v.present_value()))
                    .collect(),
            };
            changes.apply(*address, update);
        }
    }
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("database read error")]
    DatabaseRead,
    #[error("EVM execution error: {0:?}")]
    EvmError(EVMError),
    #[error("transaction failed: {0:?}")]
    TxFailed(ExecutionResult),
}
```

---

### 5. `kora-application` (New)

**Purpose:** Consensus-facing application implementing commonware traits.

**Key Traits:**

```rust
// traits.rs - Ledger trait abstraction
#[async_trait]
pub trait Ledger: Send + Sync + Clone {
    fn genesis_block(&self) -> Block;
    async fn submit_tx(&self, tx: Tx) -> bool;
    async fn build_txs(&self, max: usize, excluded: &BTreeSet<TxId>) -> Vec<Tx>;
    async fn parent_snapshot(&self, parent: ConsensusDigest) -> Option<LedgerSnapshot>;
    async fn insert_snapshot(&self, digest: ConsensusDigest, snapshot: LedgerSnapshot);
    async fn query_balance(&self, digest: ConsensusDigest, address: Address) -> Option<U256>;
    async fn persist(&self, digest: ConsensusDigest) -> Result<bool, LedgerError>;
    async fn prune_mempool(&self, txs: &[Tx]);
}
```

```rust
// ledger/mempool.rs - Mempool with owned logic
#[derive(Debug, Default)]
pub struct Mempool {
    pending: BTreeMap<TxId, Tx>,
}

impl Mempool {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, tx: Tx) -> bool {
        let id = tx.id();
        if self.pending.contains_key(&id) { return false; }
        self.pending.insert(id, tx);
        true
    }

    pub fn build(&self, max: usize, excluded: &BTreeSet<TxId>) -> Vec<Tx> {
        self.pending.iter()
            .filter(|(id, _)| !excluded.contains(id))
            .take(max)
            .map(|(_, tx)| tx.clone())
            .collect()
    }

    pub fn prune(&mut self, txs: &[Tx]) {
        for tx in txs { self.pending.remove(&tx.id()); }
    }

    pub fn len(&self) -> usize { self.pending.len() }
    pub fn is_empty(&self) -> bool { self.pending.is_empty() }
}
```

```rust
// ledger/snapshot.rs - Snapshot store with owned logic
pub struct SnapshotStore {
    genesis: ConsensusDigest,
    snapshots: HashMap<ConsensusDigest, LedgerSnapshot>,
    persisted: HashSet<ConsensusDigest>,
}

impl SnapshotStore {
    pub fn new(genesis: ConsensusDigest, snapshot: LedgerSnapshot) -> Self {
        let mut snapshots = HashMap::new();
        let mut persisted = HashSet::new();
        snapshots.insert(genesis, snapshot);
        persisted.insert(genesis);
        Self { genesis, snapshots, persisted }
    }

    pub fn get(&self, digest: &ConsensusDigest) -> Option<&LedgerSnapshot> {
        self.snapshots.get(digest)
    }

    pub fn insert(&mut self, digest: ConsensusDigest, snapshot: LedgerSnapshot) {
        self.snapshots.insert(digest, snapshot);
    }

    pub fn is_persisted(&self, digest: &ConsensusDigest) -> bool {
        self.persisted.contains(digest)
    }

    pub fn mark_persisted(&mut self, digest: ConsensusDigest) {
        self.persisted.insert(digest);
    }
}
```

```rust
// app.rs - Application implementing commonware traits
#[derive(Clone)]
pub struct KoraApplication<S, L, X: Executor = DefaultExecutor> {
    max_txs: usize,
    ledger: L,
    executor: X,
    _scheme: PhantomData<S>,
}

impl<S, L, X> KoraApplication<S, L, X>
where
    L: Ledger,
    X: Executor,
{
    pub fn new(max_txs: usize, ledger: L, executor: X) -> Self {
        Self {
            max_txs,
            ledger,
            executor,
            _scheme: PhantomData,
        }
    }

    /// Propose a new block (logic owned by type, not a global function).
    async fn propose_block<C>(
        &self,
        mut ancestry: AncestorStream<C, Block>,
    ) -> Option<Block>
    where
        C: CertScheme,
    {
        let parent = ancestry.next().await?;

        // Collect excluded tx IDs from pending ancestors
        let mut excluded = BTreeSet::new();
        for tx in &parent.txs {
            excluded.insert(tx.id());
        }
        while let Some(block) = ancestry.next().await {
            for tx in &block.txs {
                excluded.insert(tx.id());
            }
        }

        let parent_digest = parent.commitment();
        let parent_snapshot = self.ledger.parent_snapshot(parent_digest).await?;
        let height = parent.height + 1;
        let prevrandao = parent.prevrandao; // Simplified

        let txs = self.ledger.build_txs(self.max_txs, &excluded).await;

        let outcome = self.executor.execute(parent_snapshot.db, height, prevrandao, &txs).ok()?;

        Some(Block {
            parent: parent.id(),
            height,
            prevrandao,
            state_root: parent.state_root, // Updated after compute
            txs,
        })
    }

    /// Verify a proposed block (logic owned by type, not a global function).
    async fn verify_block<C>(
        &self,
        mut ancestry: AncestorStream<C, Block>,
    ) -> bool
    where
        C: CertScheme,
    {
        let block = match ancestry.next().await {
            Some(b) => b,
            None => return false,
        };
        let parent = match ancestry.next().await {
            Some(b) => b,
            None => return false,
        };

        let parent_digest = parent.commitment();
        let parent_snapshot = match self.ledger.parent_snapshot(parent_digest).await {
            Some(s) => s,
            None => return false,
        };

        let outcome = match self.executor.execute(
            parent_snapshot.db,
            block.height,
            block.prevrandao,
            &block.txs,
        ) {
            Ok(o) => o,
            Err(_) => return false,
        };

        // Verify state root matches
        // ...

        true
    }
}

impl<E, S, L, X> Application<E> for KoraApplication<S, L, X>
where
    E: Rng + Spawner + Metrics + Clock,
    S: Scheme<ConsensusDigest> + CertScheme,
    L: Ledger + 'static,
    X: Executor + 'static,
{
    type SigningScheme = S;
    type Context = Context<ConsensusDigest, PublicKey>;
    type Block = Block;

    fn genesis(&mut self) -> impl Future<Output = Self::Block> + Send {
        let block = self.ledger.genesis_block();
        async move { block }
    }

    fn propose(
        &mut self,
        context: (E, Self::Context),
        ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> impl Future<Output = Option<Self::Block>> + Send {
        let _ = context;
        self.propose_block(ancestry)
    }
}

impl<E, S, L, X> VerifyingApplication<E> for KoraApplication<S, L, X>
where
    E: Rng + Spawner + Metrics + Clock,
    S: Scheme<ConsensusDigest> + CertScheme,
    L: Ledger + 'static,
    X: Executor + 'static,
{
    fn verify(
        &mut self,
        context: (E, Self::Context),
        ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> impl Future<Output = bool> + Send {
        let _ = context;
        self.verify_block(ancestry)
    }
}
```

---

### 6. `kora-node` (New)

**Purpose:** Node wiring, configuration, and startup.

**Key Types:**

```rust
// config.rs - Configuration with validation
#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub network: NetworkConfig,
    pub consensus: ConsensusConfig,
    pub execution: ExecutionConfig,
    pub storage: StorageConfig,
}

impl NodeConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.network.validate()?;
        self.consensus.validate()?;
        self.execution.validate()?;
        self.storage.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct NetworkConfig {
    pub listen_addr: SocketAddr,
    pub bootstrap_peers: Vec<SocketAddr>,
    pub max_peers: usize,
}

impl NetworkConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_peers == 0 {
            return Err(ConfigError::InvalidMaxPeers);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ConsensusConfig {
    pub max_txs_per_block: usize,
    pub block_interval_ms: u64,
}

impl ConsensusConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_txs_per_block == 0 { return Err(ConfigError::InvalidMaxTxs); }
        if self.block_interval_ms == 0 { return Err(ConfigError::InvalidBlockInterval); }
        Ok(())
    }
}
```

```rust
// handle.rs - External handle to running node
#[derive(Clone)]
pub struct NodeHandle {
    tx_sender: mpsc::Sender<Tx>,
    query_sender: mpsc::Sender<QueryRequest>,
}

impl NodeHandle {
    pub async fn submit_tx(&self, tx: Tx) -> Result<TxId, NodeError> {
        let id = tx.id();
        self.tx_sender.send(tx).await.map_err(|_| NodeError::ChannelClosed)?;
        Ok(id)
    }

    pub async fn query_balance(&self, address: Address) -> Result<U256, NodeError> {
        let (tx, rx) = oneshot::channel();
        self.query_sender.send(QueryRequest::Balance { address, reply: tx }).await
            .map_err(|_| NodeError::ChannelClosed)?;
        rx.await.map_err(|_| NodeError::QueryFailed)?
    }
}

pub enum QueryRequest {
    Balance { address: Address, reply: oneshot::Sender<Result<U256, NodeError>> },
    StateRoot { reply: oneshot::Sender<Result<StateRoot, NodeError>> },
}
```

```rust
// channels.rs - Internal communication
pub struct NodeChannels {
    pub tx_receiver: mpsc::Receiver<Tx>,
    pub query_receiver: mpsc::Receiver<QueryRequest>,
    pub finalized_sender: broadcast::Sender<ConsensusDigest>,
}

impl NodeChannels {
    pub fn new(buffer_size: usize) -> (Self, NodeHandle) {
        let (tx_sender, tx_receiver) = mpsc::channel(buffer_size);
        let (query_sender, query_receiver) = mpsc::channel(buffer_size);
        let (finalized_sender, _) = broadcast::channel(buffer_size);

        let handle = NodeHandle { tx_sender, query_sender };
        let channels = Self { tx_receiver, query_receiver, finalized_sender };
        (channels, handle)
    }
}
```

---

## Implementation Order

### Phase 1: Core Types
- Expand `kora-primitives` with chain constants
- Create `kora-domain` with Block, Tx, ID types
- Implement codec (Write/Read) for all types
- Add validation methods with unit tests

### Phase 2: Storage Layer
- Create `kora-storage` with trait definitions
- Implement REVM database adapter
- Implement change set tracking
- Add in-memory storage for testing

### Phase 3: Execution Layer
- Create `kora-execution` with EVM environment builder
- Add custom precompiles (KoraPrecompiles type)
- Implement DefaultExecutor
- Add comprehensive execution tests

### Phase 4: Application Layer
- Create `kora-application` with Ledger trait
- Implement Mempool and SnapshotStore
- Implement KoraApplication traits
- Add application tests

### Phase 5: Node Wiring
- Create `kora-node` with configuration types
- Implement channels and NodeHandle
- Implement node startup orchestration
- Add integration tests

---

## Verification Plan

### Unit Tests
Each crate should have comprehensive unit tests for:
- Type construction and validation
- Serialization roundtrips (encode/decode)
- ID computation stability
- Trait implementations

### Integration Tests
- Execute transactions against in-memory storage
- Verify state changes propagate correctly
- Test mempool operations
- Test snapshot store operations

### End-to-End Tests
```bash
# Run unit tests
cargo test -p kora-primitives
cargo test -p kora-domain
cargo test -p kora-storage
cargo test -p kora-execution
cargo test -p kora-application
cargo test -p kora-node

# Run all tests
cargo test --workspace

# Run with release optimizations
cargo test --workspace --release
```

---

## Key Files to Create/Modify

1. `/Users/andreasbigger/kora/crates/domain/src/types/block.rs` - Block type
2. `/Users/andreasbigger/kora/crates/domain/src/types/tx.rs` - Tx type
3. `/Users/andreasbigger/kora/crates/storage/src/traits.rs` - Storage traits
4. `/Users/andreasbigger/kora/crates/execution/src/executor.rs` - Executor
5. `/Users/andreasbigger/kora/crates/application/src/app.rs` - Application
6. `/Users/andreasbigger/kora/crates/application/src/ledger/mod.rs` - Ledger
7. `/Users/andreasbigger/kora/crates/node/src/config.rs` - Configuration
