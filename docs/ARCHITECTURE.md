# Architecture

## Core principles

`loncher` is a daemon-first Linux/Niri application. UI is an optional frontend adapter, not the owner of state.

```
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

The UI boundary must expose snapshots/events only:

```
services -> UiSnapshot -> frontend
frontend -> UiEvent -> services
```

Iced/layer-shell is an implementation detail.

## Sync

Sync is a first-class capability, not a GUI feature.

Initial goals:

- device identity
- encrypted operation log
- settings/memory synchronization
- explicit scopes
- offline support

Do not sync:

- secrets
- sockets
- temporary runtime state
- hardware-specific state

## Agent

MCP is the agent-facing capability boundary.

UI and agent use the same services:

```
UI -> typed Rust services
AI -> MCP adapters -> typed Rust services
```

Memory model:

- working
- episodic
- semantic
- procedural

FTS5-first retrieval. Embeddings only after measured need.
