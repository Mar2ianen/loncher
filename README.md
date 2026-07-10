# loncher

`loncher` — Linux/Niri-first desktop control plane на Rust: быстрый launcher, системная панель, файловый поиск и preview, терминал, а позднее — MCP-агент с долговременной памятью и диктовкой.

Ключевой принцип: сначала полезный локальный launcher, затем AI поверх уже работающих capabilities.

## Инварианты

- Один долгоживущий пользовательский daemon и один release binary.
- UI не владеет долговременным состоянием: daemon скрывает/показывает layer-shell surface.
- Rust services — источник истины.
- UI вызывает typed Rust API напрямую.
- Агент получает те же capabilities через MCP.
- Linux/Niri — первая целевая платформа.
- Никаких `panic!`, `unwrap()` и `expect()` в production-коде.

## Планируемый стек

- Runtime: Tokio, `tokio-util`.
- GUI: `iced` + layer-shell integration.
- IPC: Unix domain socket.
- Search: `nucleo`, `ignore`, `notify`.
- Storage: SQLite/FTS5 через `rusqlite`.
- Desktop: `.desktop`/Freedesktop icons, Niri IPC.
- System: D-Bus/`zbus`, NetworkManager, BlueZ, PipeWire/WirePlumber, MPRIS.
- Terminal: адаптер над `alacritty_terminal`.
- Agent: OpenAI-compatible provider layer, Cerebras, `rmcp`, Hermes-like memory.
- Dictation: Groq STT + локальный Whisper с fallback/hedged policy.

## Репозиторий

- [`docs/ROADMAP.md`](docs/ROADMAP.md) — общий план проекта.
- [`docs/PHASE-0-DAEMON.md`](docs/PHASE-0-DAEMON.md) — подробный план первого шага.
- [`AGENTS.md`](AGENTS.md) — правила для кодовых агентов и стиль проекта.

## Быстрый старт

```bash
cp .env.example .env
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p loncher -- daemon
```

CLI-каркас:

```bash
cargo run -p loncher -- toggle
cargo run -p loncher -- show
cargo run -p loncher -- hide
cargo run -p loncher -- query zed
cargo run -p loncher -- agent "проверь состояние systemd unit"
```

## Статус

Инициализация каркаса. Первый вертикальный срез: daemon + IPC + show/hide/toggle без AI.
