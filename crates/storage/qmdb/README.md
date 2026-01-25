# `kora-qmdb`

<a href="https://github.com/anthropics/kora/actions/workflows/ci.yml"><img src="https://github.com/anthropics/kora/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
<a href="https://github.com/anthropics/kora/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-d1d1f6.svg" alt="License"></a>

Minimal QMDB adapter implementing REVM's `Database` trait for the Kora execution client.

## Overview

This crate provides a simple adapter bridging QMDB (Quick Merkle Database) to REVM's Database
trait. Following commonware's REVM example patterns, the implementation is intentionally minimal
(~150 lines) and delegates to QMDB's built-in APIs.

Key components:

- **`QmdbDatabase`**: Wraps QMDB stores and implements REVM's `DatabaseRef` and `DatabaseCommit`.
- **`QmdbError`**: Simple error type implementing REVM's `DBErrorMarker`.
- **Encoding helpers**: Functions for account and storage key serialization.

## Architecture

```text
┌─────────────────────────────────┐
│         REVM Executor           │
└───────────────┬─────────────────┘
                │ DatabaseRef / DatabaseCommit
                ▼
┌─────────────────────────────────┐
│        QmdbDatabase             │
│  (accounts, storage, code)      │
└───────────────┬─────────────────┘
                │ Gettable / Batchable
                ▼
┌─────────────────────────────────┐
│         QMDB Stores             │
└─────────────────────────────────┘
```

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
kora-qmdb = { path = "crates/storage/qmdb" }
```

Use with REVM:

```rust,ignore
use kora_qmdb::QmdbDatabase;

// Create from QMDB stores
let db = QmdbDatabase::new(accounts_store, storage_store, code_store);

// Use with REVM
let mut evm = Evm::builder().with_db(db).build();
let result = evm.transact()?;

// Commit changes using QMDB's built-in batching
evm.db_mut().commit(result.state);
```

## License

[MIT License](https://github.com/anthropics/kora/blob/main/LICENSE)
