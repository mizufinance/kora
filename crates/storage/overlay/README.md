# kora-overlay

Overlay state helpers for Kora.

This crate provides an `OverlayState` wrapper that layers an in-memory
change set on top of a base `StateDb`. It is used to execute blocks and
compute roots against unpersisted ancestor changes.

## Key Types

- `OverlayState` - StateDb implementation that merges a base state with pending changes

## Usage

```rust,ignore
use kora_overlay::OverlayState;

let overlay = OverlayState::new(base_state, pending_changes);
let balance = overlay.balance(&address).await?;
```
