# `kora-dkg`

[![CI](https://github.com/refcell/kora/actions/workflows/ci.yml/badge.svg)](https://github.com/refcell/kora/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Interactive Distributed Key Generation (DKG) for Kora threshold cryptography.

Implements the Joint-Feldman DKG protocol using [commonware-cryptography](https://docs.rs/commonware-cryptography) BLS12-381 primitives. Each participant contributes to the key generation process, and no single party learns the master secret.

## Key Types

- `DkgCeremony` - Orchestrates the full interactive DKG ceremony
- `DkgParticipant` - State machine for a participant (both dealer and player)
- `DkgConfig` - Configuration for DKG parameters
- `DkgNetwork` - TCP-based networking for DKG messages
- `DkgOutput` - Output containing generated keys and shares
- `DkgError` - Error types for DKG operations
- `ProtocolMessage` - Message types for the DKG protocol

## Protocol Flow

1. **Setup**: Each node loads its Ed25519 identity key and participant list
2. **Dealer Phase**: Each node generates commitment polynomials and shares for all players
3. **Player Phase**: Each node verifies received shares and sends acknowledgements
4. **Finalization**: Dealers create signed logs, leader coordinates transcript agreement
5. **Output**: Each node computes its BLS12-381 share and the group public key

## License

[MIT License](https://opensource.org/licenses/MIT)
