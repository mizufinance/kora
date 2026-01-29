# Kora Docker Devnet

This directory contains Docker configurations for running a local Kora devnet with interactive DKG (Distributed Key Generation).

## Quick Start

From the repository root:

```bash
just devnet
```

This will:
1. Build the Docker image
2. Generate validator identity keys (Phase 0)
3. Run interactive DKG ceremony to establish threshold cryptography (Phase 1)
4. Start 4 validator nodes with threshold BLS consensus (Phase 2)
5. Start Prometheus and Grafana for observability

## Commands

| Command | Description |
|---------|-------------|
| `just devnet` | Start the full devnet with observability |
| `just devnet-down` | Stop all containers (preserves state) |
| `just devnet-reset` | Stop and delete all state (fresh DKG on next start) |
| `just devnet-status` | Show container status and endpoints |
| `just devnet-logs` | Stream validator logs |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Kora Devnet                              │
├─────────────────────────────────────────────────────────────────┤
│  Phase 0: init-config                                           │
│    - Generates ed25519 identity keys                            │
│    - Creates peers.json and genesis.json                        │
├─────────────────────────────────────────────────────────────────┤
│  Phase 1: DKG Ceremony (dkg-node0..3)                          │
│    - Interactive threshold key generation                       │
│    - Uses Ed25519 simplex consensus                             │
│    - Outputs: output.json + share.key per node                 │
├─────────────────────────────────────────────────────────────────┤
│  Phase 2: Validators (validator-node0..3)                       │
│    - BLS12-381 threshold consensus                              │
│    - REVM execution                                             │
│    - QMDB state storage                                         │
├─────────────────────────────────────────────────────────────────┤
│  Observability: prometheus + grafana                            │
│    - Prometheus: http://localhost:9090                          │
│    - Grafana: http://localhost:3000 (admin/admin)              │
└─────────────────────────────────────────────────────────────────┘
```

## Endpoints

| Service | Port | Description |
|---------|------|-------------|
| validator-node0 P2P | 30400 | P2P networking |
| validator-node1 P2P | 30401 | P2P networking |
| validator-node2 P2P | 30402 | P2P networking |
| validator-node3 P2P | 30403 | P2P networking |
| RPC (all nodes) | 8540-8543 | Reserved (Phase 3) |
| Prometheus | 9090 | Metrics |
| Grafana | 3000 | Dashboards |

## Configuration

Environment variables (set in `.env` or export):

| Variable | Default | Description |
|----------|---------|-------------|
| `CHAIN_ID` | 1337 | Chain identifier |
| `RUST_LOG` | info | Log level |
| `COMPOSE_PROFILES` | observability | Enable observability stack |

## Directory Structure

```
docker/
├── Dockerfile              # Multi-stage build with cargo-chef
├── docker-bake.hcl         # Buildx configuration
├── Justfile                # Command runner
├── compose/
│   └── devnet.yaml         # Docker Compose configuration
├── config/
│   └── prometheus.yml      # Prometheus scrape config
├── scripts/
│   ├── entrypoint.sh       # Container entrypoint
│   └── healthcheck.sh      # Health check script
└── grafana/
    ├── provisioning/       # Auto-configure datasources
    └── dashboards/         # Pre-built dashboards
```

## Development

Build the image locally:

```bash
cd docker
just build
```

Validate compose file:

```bash
just validate
```

Re-run DKG (keeps other state):

```bash
just redo-dkg
```

## Troubleshooting

**DKG doesn't complete:**
- Check logs: `just logs-dkg`
- Ensure all 4 DKG nodes can reach each other
- Increase timeout if network is slow

**Validators crash on startup:**
- Verify DKG completed: check for `share.key` in data volumes
- Check logs: `just logs`

**Full reset:**
```bash
just reset
just devnet
```
