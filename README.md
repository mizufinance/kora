<p align="center">
    <img src="./assets/logo.png" alt="Kora" width="300">
</p>

<h1 align="center">Kora</h1>

<h4 align="center">
    A minimal commonware + revm execution client. Built in Rust.
</h4>

<p align="center">
  <img src="https://img.shields.io/crates/msrv/kora?style=flat&labelColor=1C2C2E&color=fbbf24&logo=rust&logoColor=white" alt="MSRV">
  <a href="https://github.com/refcell/kora/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/refcell/kora/ci.yml?style=flat&labelColor=1C2C2E&label=ci&logo=GitHub%20Actions&logoColor=white" alt="CI"></a>
  <a href="https://github.com/refcell/kora/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-d1d1f6.svg?style=flat&labelColor=1C2C2E&color=a78bfa&logo=googledocs&logoColor=white" alt="License"></a>
</p>

<p align="center">
  <a href="#whats-kora">What's Kora?</a> •
  <a href="#usage">Usage</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#contributing">Contributing</a> •
  <a href="#license">License</a>
</p>

> [!CAUTION]
> Kora is pre-alpha software. Do not expect this to run. Do not run this in production.

> [!IMPORTANT]
> Kora uses BLS12-381 threshold consensus via [commonware](https://github.com/commonwarexyz/monorepo), [REVM](https://github.com/bluealloy/revm) for EVM execution, and [QMDB](https://github.com/kora-labs/qmdb) for state storage.

## What's Kora?

Kora is a minimal, high-performance execution client built entirely in Rust. It combines commonware's BLS12-381 threshold consensus with REVM for EVM execution and QMDB for efficient state storage.

The architecture is modular with distinct layers:
- **Consensus** - BLS12-381 threshold signatures via commonware simplex
- **Execution** - EVM state transitions powered by REVM
- **Storage** - High-performance state management with QMDB
- **Network** - P2P transport and message marshaling

## Usage

Start the devnet with interactive DKG (Distributed Key Generation):

```sh
just devnet
```

> [!TIP]
> See the [Justfile](./Justfile) for other useful commands.

### Devnet Commands

| Command | Description |
|---------|-------------|
| `just devnet` | Start the devnet with interactive DKG |
| `just devnet-down` | Stop the devnet |
| `just devnet-reset` | Reset devnet (clears all state, requires fresh DKG) |
| `just devnet-logs` | View devnet logs |
| `just devnet-status` | View devnet status |
| `just devnet-stats` | Live devnet monitoring dashboard |

## Architecture

The devnet runs in three phases:

### Phase 0: Init Config
Generates ed25519 identity keys for each validator node.

### Phase 1: DKG Ceremony
Interactive threshold key generation using Ed25519 simplex consensus. Validators collaborate to generate a shared BLS12-381 threshold key.

### Phase 2: Validators
Full validator nodes running:
- BLS12-381 threshold consensus
- REVM execution engine
- QMDB state storage

### Observability
Prometheus metrics with Grafana dashboards for monitoring.

### Crate Structure

```
bin/
├── kora          # Main validator binary
└── keygen        # Key generation for devnet

crates/
├── node/         # Node components
│   ├── consensus # BLS threshold consensus
│   ├── executor  # REVM execution
│   ├── ledger    # Block/transaction ledger
│   ├── txpool    # Transaction pool
│   ├── service   # Node service orchestration
│   ├── dkg       # Distributed key generation
│   └── ...
├── storage/      # Storage layer
│   ├── qmdb      # QMDB integration
│   ├── backend   # Storage backend
│   ├── handlers  # State handlers
│   ├── overlay   # State overlay
│   └── ...
├── network/      # Network layer
│   ├── transport # P2P transport
│   └── marshal   # Message serialization
└── utilities/    # Utilities
    ├── cli       # CLI helpers
    ├── crypto    # Cryptographic utilities
    └── sys       # System utilities
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
