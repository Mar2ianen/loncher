# loncher

`loncher` — Linux/Niri-first desktop control plane на Rust: быстрый launcher, системная панель, файловый поиск и preview, терминал, а позднее — MCP-агент с долговременной памятью и диктовкой.

Ключевой принцип: сначала полезный локальный launcher, затем AI поверх уже работающих capabilities.

## Инварианты

- Один долгоживущий пользовательский daemon и один release binary.
- GUI — опциональный frontend adapter; daemon/core обязаны собираться и работать без GUI.
- UI не владеет долговременным состоянием.
- Rust services — источник истины.
- UI вызывает typed Rust API напрямую.
- Агент получает те же capabilities через MCP.
- Sync — отдельный first-class service со своим протоколом, а не часть GUI и не MCP transport.
- Linux/Niri — первая целевая платформа.
- Никаких `panic!`, `unwrap()` и `expect()` в production-коде.

## Планируемый стек

- Runtime: Tokio, `tokio-util`.
- GUI: `iced` + layer-shell integration за `ui-contract`.
- IPC: Unix domain socket.
- Search: `nucleo`, `ignore`, `notify`.
- Storage: SQLite/FTS5 через `rusqlite`.
- Desktop: `.desktop`/Freedesktop icons, Niri IPC.
- System: D-Bus/`zbus`, NetworkManager, BlueZ, PipeWire/WirePlumber, MPRIS.
- Terminal: адаптер над `alacritty_terminal`.
- Sync: versioned operation log, per-device cursors, application-level encryption, HTTP transport first.
- Agent: OpenAI-compatible provider layer, Cerebras, `rmcp`, Hermes-like memory.
- Dictation: Groq STT + локальный Whisper с fallback/hedged policy.

## Репозиторий

- [`docs/ROADMAP.md`](docs/ROADMAP.md) — общий план проекта.
- [`docs/PHASE-0-DAEMON.md`](docs/PHASE-0-DAEMON.md) — подробный план первого шага.
- [`docs/SYNC.md`](docs/SYNC.md) — границы, scopes и протокол синхронизации.
- [`AGENTS.md`](AGENTS.md) — правила для кодовых агентов и стиль проекта.

## Сборки

Сейчас default build — headless daemon skeleton. GUI будет добавлен как optional feature, а не протечёт в core dependency graph.

```bash
cargo build -p loncher --no-default-features
cargo build -p loncher --no-default-features --features sync-client
cargo build -p loncher --no-default-features --features desktop
cargo build -p loncher --all-features
```

Отсутствие feature `gui` означает headless build. Отдельного отрицательного feature `headless` нет.

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

Инициализирован daemon-first workspace, GUI contract и sync protocol/engine skeleton. Первый настоящий вертикальный срез остаётся прежним: daemon + IPC + show/hide/toggle без AI.
