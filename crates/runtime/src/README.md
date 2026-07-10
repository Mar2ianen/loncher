# Runtime internals

`config.rs` resolves the Phase 0 socket and limits. `transport.rs` owns framing. `server.rs` owns listener, peer checks, connection tasks and the serialized command router. `lib.rs` exposes daemon and one-request client entry points.
