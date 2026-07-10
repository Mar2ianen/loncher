# Architecture

## Core principles

`loncher` is a daemon-first Linux/Niri application. UI is an optional frontend adapter, not the owner of state.

```text
daemon core
  ├── services
  ├── storage
  ├── agent runtime
  ├── MCP host
  ├── sync engine
  └── frontend adapters
```

## Frontends

The core must compile without GUI.

The UI boundary exposes daemon-owned snapshots and frontend-originated events only:

```text
services -> UiSnapshot -> frontend
frontend -> UiEvent -> services
```

The frontend does not expose an authoritative snapshot back to the daemon. Iced/layer-shell is an implementation detail.

A headless backend accepts hidden snapshots and shutdown, but rejects snapshots that require a visible surface with a typed `UnavailableInBuild` error.

## Sync

Sync is a first-class capability, not a GUI feature.

Initial goals:

- device identity
- encrypted operation log
- settings/memory synchronization
- explicit scopes
- offline support

Protocol invariants:

- identifiers are validated during deserialization
- `operation_id` is idempotent only for identical operation content
- reusing an operation ID for different content is an integrity error
- each device sequence is gap-free and accepted in order
- cursors advance independently per device

Do not sync:

- secrets
- sockets
- temporary runtime state
- hardware-specific state

## Agent

MCP is the agent-facing capability boundary.

UI and agent use the same services:

```text
UI -> typed Rust services
AI -> MCP adapters -> typed Rust services
```

Memory model:

- working
- episodic
- semantic
- procedural

FTS5-first retrieval. Embeddings only after measured need.
