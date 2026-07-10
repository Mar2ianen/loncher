# Phase 1A — applications, search and minimal GUI

Phase 1A turns the Phase 0 daemon into a launcher without adding history,
ranking, SQLite, Niri IPC or file search.

## Boundaries

- `applications` owns XDG discovery, desktop IDs, locale selection, icon
  references, diagnostics and shell-free launch argv.
- `search` owns the high-level `nucleo` index and bounded result snapshots.
- `ui-contract` contains only framework-neutral snapshots, application view
  models and input events.
- `ui-iced` is the optional Iced/layer-shell leaf adapter.
- `runtime` owns discovery startup, the search service and UI projection.

Discovery runs outside Tokio worker threads. User data directories take
precedence over system data directories; the first desktop ID wins. Invalid
entries produce structured diagnostics and do not abort the catalog.

The launcher uses `nucleo 0.5` for Unicode-aware fuzzy matching over name,
generic name, keywords and desktop ID. Empty queries use deterministic name
ordering and the UI receives at most twelve results. Search generations prevent
an older asynchronous response from replacing a newer query.

Launching uses parsed argv and `std::process::Command`; it never passes an
Exec field through a shell. Terminal entries and D-Bus activation are explicit
unsupported errors in this slice.

## Dependencies and MSRV

The workspace baseline is Rust 1.88. `iced 0.14` requires Rust 1.88, while
`freedesktop-desktop-entry 0.8.1` and `nucleo 0.5.0` are isolated behind the
application/search boundaries. `Cargo.lock` is committed.

## Verification

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check -p loncher --no-default-features
```
