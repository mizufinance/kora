# kora-backend

Commonware-storage based backend for Kora QMDB.

This crate provides storage implementations using commonware-storage primitives for the three QMDB partitions:
- **AccountStore**: Stores account state (nonce, balance, code hash, generation)
- **StorageStore**: Stores contract storage slots
- **CodeStore**: Stores contract bytecode

## Usage

```rust,ignore
use kora_backend::{CommonwareBackend, QmdbBackendConfig};
use std::path::PathBuf;

// Create an in-memory backend
let backend = CommonwareBackend::new();

// Or open with configuration
let config = QmdbBackendConfig::new(PathBuf::from("/path/to/data"), 1_000_000);
let backend = CommonwareBackend::open(config).await?;

// Get state root
let root = backend.get_state_root().await?;
```
