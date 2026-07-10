# Phase 0 implementation status

This document records the implemented daemon/IPC vertical slice. The normative design and acceptance criteria remain in `PHASE-0-DAEMON.md`.

## Implemented

- one daemon process and one CLI client binary;
- `$XDG_RUNTIME_DIR/loncher/loncher.sock` with explicit `LONCHER_SOCKET` override;
- newly created parent mode `0700`, existing private-parent validation, socket mode `0600`, final-path symlink rejection;
- stale-socket connect probe and safe recovery;
- peer UID validation;
- versioned request/reply envelopes with request IDs;
- `LengthDelimitedCodec` framing and JSON payloads;
- bounded `mpsc` command router and `oneshot` replies;
- pure daemon state reducer with generation tracking;
- typed public protocol errors, including `ui_unavailable`;
- bounded connection tasks, per-request deadline, cancellation and socket cleanup;
- `SIGINT`, `SIGTERM` and IPC shutdown paths;
- `systemd --user` unit example;
- integration tests for round trips, headless rejection, concurrent clients, second-daemon rejection, stale recovery, malformed/versioned frames, idle-client deadlines, permissions and cleanup.

## Intentionally deferred

- real Iced/layer-shell frontend;
- configuration file schema beyond environment-based Phase 0 values;
- richer per-request latency metrics;
- explicit queue-overflow rejection policy beyond bounded backpressure;
- production launcher/search services.
