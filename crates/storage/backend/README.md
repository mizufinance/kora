# `kora-backend`

<a href="https://github.com/refcell/kora/actions/workflows/ci.yml"><img src="https://github.com/refcell/kora/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<a href="https://github.com/refcell/kora/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-d1d1f6.svg" alt="License"></a>

Concrete storage backend for Kora QMDB.

This crate implements the `QmdbGettable` and `QmdbBatchable` traits from [`kora-qmdb`](../qmdb) with in-memory stores:

- **AccountStore** - Account state (nonce, balance, code hash, generation)
- **StorageStore** - Contract storage slots
- **CodeStore** - Contract bytecode

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

## License

[MIT License](https://github.com/refcell/kora/blob/main/LICENSE)
