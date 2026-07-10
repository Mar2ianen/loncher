# Phase 0: daemon foundation

## Результат фазы

После завершения Phase 0 один процесс `loncher daemon` постоянно живёт в пользовательской сессии. Повторный вызов того же binary работает как CLI-клиент, подключается к Unix socket, отправляет команду и завершается.

Полноценный Iced UI, search index, Niri integration, MCP и AI в эту фазу не входят.

## Acceptance criteria

1. `loncher daemon` создаёт единственный daemon instance.
2. `loncher show`, `hide`, `toggle`, `query <text>` и `agent <prompt>` подключаются к нему через Unix socket.
3. Второй `loncher daemon` не создаёт второй daemon и возвращает понятную ошибку/статус.
4. `Ctrl+C` и `systemctl --user stop loncher` корректно завершают listener и удаляют socket.
5. После kill/crash stale socket безопасно восстанавливается при следующем запуске.
6. Socket недоступен другим пользователям.
7. Protocol errors не валят daemon и возвращают typed error reply.
8. Все queues bounded; все spawned tasks принадлежат runtime и завершаются через cancellation.
9. `cargo fmt`, `clippy -D warnings` и tests проходят.

## Process model

```text
systemd --user
    └── loncher daemon
          ├── Unix socket listener
          ├── command router
          ├── daemon state
          ├── service registry (пока пустой)
          └── optional UI handle (stub)

Niri hotkey / shell
    └── loncher toggle
          └── connect → request → reply → exit
```

Не держать отдельный tray process, IPC helper или agent daemon.

## CLI contract

```text
loncher daemon
loncher show [--query <text>]
loncher hide
loncher toggle [--query <text>]
loncher query <text>
loncher agent <prompt>
loncher status
```

На этой фазе `query` и `agent` только передают намерение в daemon state/UI stub. Они нужны, чтобы protocol не пришлось ломать на следующей фазе.

## IPC path

Default:

```text
$XDG_RUNTIME_DIR/loncher/loncher.sock
```

Fallback при отсутствии `XDG_RUNTIME_DIR` не придумывать молча. Daemon должен вернуть понятную ошибку с инструкцией. Для ручной разработки допустим явный `LONCHER_SOCKET`.

Требования:

- parent directory mode `0700`;
- socket доступен только владельцу;
- symlink path запрещён;
- перед удалением stale socket выполнить connect probe;
- active socket никогда не удалять;
- temporary bind/rename применять только если это реально поддерживается выбранной схемой Unix socket.

## Protocol

Transport: Unix stream socket.

Framing: length-delimited frames через `tokio_util::codec::LengthDelimitedCodec`.

Payload: versioned JSON на первом этапе. Inspectability сейчас полезнее бинарного protocol; transport framing позволяет позднее заменить encoding без изменения socket semantics.

```rust
struct RequestEnvelope {
    protocol_version: u16,
    request_id: RequestId,
    command: DaemonCommand,
}

struct ReplyEnvelope {
    protocol_version: u16,
    request_id: RequestId,
    result: Result<DaemonReply, ProtocolError>,
}
```

Initial commands:

```rust
enum DaemonCommand {
    Show { query: Option<String> },
    Hide,
    Toggle { query: Option<String> },
    Query { text: String },
    OpenAgent { prompt: Option<String> },
    Status,
    Shutdown,
}
```

`Shutdown` не публиковать обычному UI без отдельной CLI-команды и проверки локального пользователя.

## Daemon state

```rust
struct DaemonState {
    ui: UiVisibility,
    active_query: Option<String>,
    active_mode: LauncherMode,
    generation: u64,
}

enum UiVisibility {
    Hidden,
    Showing,
    Visible,
    Hiding,
}

enum LauncherMode {
    Launcher,
    Terminal,
    Agent,
}
```

Даже пока UI stub, state transitions должны быть отдельной чистой функцией с unit tests.

Инварианты:

- `Hide` идемпотентен;
- `Show` идемпотентен и может обновить query;
- `Toggle` определяется текущим logical state, а не наличием Wayland object;
- generation увеличивается при observable state change;
- invalid empty query/prompt отклоняется до изменения state.

## Runtime structure

```text
main
├── load environment/config
├── init tracing
├── parse CLI
├── daemon path
│   ├── acquire instance ownership
│   ├── create cancellation token
│   ├── bind socket
│   ├── start listener task
│   ├── start command router task
│   ├── wait for signal/fatal error
│   └── cancel → join tasks → remove socket
└── client path
    ├── connect with timeout
    ├── send one request
    ├── receive one reply
    └── exit with mapped status code
```

Рекомендуемые primitives:

- bounded `mpsc` для command router;
- `oneshot` для reply конкретному IPC request;
- `watch` для daemon snapshot будущим subscribers;
- `CancellationToken` как корневой shutdown signal;
- `JoinSet` или явный task registry для owned tasks.

Не использовать detached `tokio::spawn`.

## Error model

Минимальные категории:

```rust
enum DaemonError {
    RuntimeDirUnavailable,
    InstanceAlreadyRunning,
    SocketPathUnsafe,
    Bind,
    Permission,
    Protocol,
    Io,
    Task,
}

enum ProtocolError {
    UnsupportedVersion,
    InvalidFrame,
    InvalidCommand,
    RequestTooLarge,
    Internal,
}
```

Internal details логируются с request ID, клиент получает стабильный public code без secrets/path leakage.

## Tracing

Каждый request получает span:

```text
request_id
command_kind
peer_uid
queue_wait_ms
handler_ms
total_ms
result_code
```

Не логировать query/prompt body по умолчанию. Разрешить opt-in debug body logging только отдельным development flag с явным предупреждением.

## Config

На Phase 0:

```text
socket path override
request timeout
max frame size
command queue capacity
log filter
```

Приоритет:

```text
CLI > environment > XDG config > defaults
```

Secrets отсутствуют в XDG config schema. `.env` используется только локально и игнорируется Git.

## systemd user unit

Unit хранится в `packaging/systemd/loncher.service`.

Требования:

- `Type=simple`;
- `Restart=on-failure`;
- разумный restart delay;
- остановка через SIGTERM;
- без shell wrapper;
- environment overrides через `%h/.config/loncher/environment`, если файл существует;
- unit не должен зависеть от graphical target сильнее необходимого до появления Wayland UI.

## Предлагаемая разбивка commits

### 0.1 Domain protocol

- Request/reply envelopes.
- Command validation.
- Pure state reducer.
- Unit tests.

### 0.2 Paths and instance ownership

- XDG runtime path.
- Secure directory/socket checks.
- Stale socket probe.
- Integration tests с temporary directory.

### 0.3 Server/client transport

- Length-delimited codec.
- Listener accept loop.
- One-request client.
- Frame/version/error tests.

### 0.4 Runtime lifecycle

- Command router.
- Cancellation tree.
- Task ownership/join.
- Signals and cleanup.

### 0.5 systemd and smoke tests

- User unit.
- CLI exit codes.
- End-to-end daemon/client test.
- Documentation update.

## Tests

### Unit

- command validation;
- reducer transitions and idempotency;
- protocol version check;
- path validation;
- public error mapping.

### Integration

- start daemon on temporary socket;
- send `Show`, verify state/reply;
- concurrent clients;
- malformed/oversized frame;
- second daemon rejected;
- stale socket recovered;
- shutdown removes socket;
- queue saturation has deterministic behavior.

### Smoke

```bash
cargo run -p loncher -- daemon &
DAEMON_PID=$!
cargo run -p loncher -- status
cargo run -p loncher -- toggle
cargo run -p loncher -- query zed
kill -TERM "$DAEMON_PID"
wait "$DAEMON_PID"
```

## Out of scope

- Iced rendering and layer-shell.
- `.desktop` parsing/search.
- Niri IPC.
- SQLite.
- Terminal/PTY.
- MCP/provider APIs.
- Agent memory.
- Dictation.

Не добавлять stubs этих подсистем глубже, чем требуется для стабильных domain contracts.
