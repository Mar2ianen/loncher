# Daemon IPC protocol

Phase 0 uses one request and one reply per Unix stream connection.

- path: `$XDG_RUNTIME_DIR/loncher/loncher.sock`;
- framing: `tokio_util::codec::LengthDelimitedCodec`;
- payload: versioned JSON;
- request identity: non-zero process-local `u64` echoed by the daemon;
- maximum frame: 1 MiB by default;
- queue: bounded daemon command channel;
- connections: active connection tasks are capped at the command queue capacity;
- deadline: one request must complete within the configured request timeout;
- authentication: socket permissions plus peer UID validation.

The daemon never logs query text or agent prompts by default. Public errors contain stable codes and omit internal paths and error chains.
