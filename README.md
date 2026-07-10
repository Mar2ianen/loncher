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

## Текущий вертикальный срез

Phase 0 реализует один daemon process и CLI-клиент в том же binary:

```text
systemd --user / shell
        └── loncher daemon
              ├── owner-only Unix socket
              ├── versioned request/reply protocol
              ├── bounded command router
              ├── daemon-owned state reducer
              └── optional UiBackend

Niri hotkey / shell
        └── loncher toggle
              └── connect → request → reply → exit
```

Socket по умолчанию: `$XDG_RUNTIME_DIR/loncher/loncher.sock`. Для тестов и разработки разрешён явный `LONCHER_SOCKET`.

## Планируемый стек

- Runtime: Tokio, `tokio-util`.
- GUI: `iced` + layer-shell integration за `ui-contract`.
- IPC: Unix domain socket, length-delimited versioned JSON.
- Search: `nucleo`, `ignore`, `notify`.
- Storage: SQLite/FTS5 через `rusqlite`.
- Desktop: `.desktop`/Freedesktop icons, Niri IPC.
- System: D-Bus/`zbus`, NetworkManager, BlueZ, PipeWire/WirePlumber, MPRIS.
- Terminal: адаптер над `alacritty_terminal`.
- Sync: versioned operation log, per-device cursors, application-level encryption, HTTP transport first.
- Agent: OpenAI-compatible provider layer, Cerebras, `rmcp`, Hermes-like memory.
- Dictation: Groq STT + локальный Whisper с fallback/hedged policy.

## Сборки

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

В другом shell:

```bash
cargo run -p loncher -- status
cargo run -p loncher -- hide
cargo run -p loncher -- shutdown
```

`show`, `toggle`, `query` и `agent` в headless build возвращают typed `ui_unavailable`. После подключения GUI backend те же команды начнут управлять surface без изменения IPC-контракта.

## systemd --user

Пример unit находится в [`packaging/systemd/loncher.service`](packaging/systemd/loncher.service).

```bash
install -Dm644 packaging/systemd/loncher.service \
  ~/.config/systemd/user/loncher.service
systemctl --user daemon-reload
systemctl --user enable --now loncher.service
```

Unit ожидает binary в `~/.local/bin/loncher`.

## Документация

- [`docs/ROADMAP.md`](docs/ROADMAP.md) — общий план проекта.
- [`docs/PHASE-0-DAEMON.md`](docs/PHASE-0-DAEMON.md) — acceptance criteria daemon/IPC.
- [`docs/IPC-PROTOCOL.md`](docs/IPC-PROTOCOL.md) — wire format и trust boundary.
- [`docs/REVIEW-PHASE-0.md`](docs/REVIEW-PHASE-0.md) — чеклист ревью.
- [`docs/SYNC.md`](docs/SYNC.md) — границы, scopes и протокол синхронизации.
- [`AGENTS.md`](AGENTS.md) — правила для кодовых агентов и стиль проекта.
