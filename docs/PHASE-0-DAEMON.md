# Phase 0: daemon foundation

## Результат фазы

После завершения Phase 0 один процесс `loncher daemon` постоянно живёт в пользовательской сессии. Повторный вызов того же binary работает как CLI-клиент, подключается к Unix socket, отправляет команду и завершается.

Полноценный Iced UI, search index, Niri integration, MCP и AI в эту фазу не входят. GUI существует только как `UiBackend` contract; default/headless build не линкует GUI framework.

## Acceptance criteria

1. `loncher daemon` создаёт единственный daemon instance.
2. `loncher show`, `hide`, `toggle`, `query <text>` и `agent <prompt>` подключаются к нему через Unix socket.
3. Второй daemon не создаёт второй instance.
4. `Ctrl+C` и `systemctl --user stop loncher` корректно завершают tasks и удаляют socket.
5. Stale socket безопасно восстанавливается.
6. Socket недоступен другим пользователям.
7. Protocol errors возвращают typed reply и не валят daemon.
8. Все queues bounded; spawned tasks принадлежат runtime и завершаются через cancellation.
9. Runtime зависит от `ui-contract`, но не от Iced/Wayland.
10. `cargo build -p loncher --no-default-features` проходит.
11. `show/toggle` в headless build возвращают typed `UiUnavailable`, а не silent success.
12. fmt, clippy и tests проходят во всей feature matrix.

## Process model

```text
systemd --user
    └── loncher daemon
          ├── Unix socket listener
          ├── command router
          ├── daemon state
          ├── service registry
          └── UiBackend
                ├── UnavailableUiBackend
                └── future IcedUiBackend

Niri hotkey / shell
    └── loncher toggle
          └── connect → request → reply → exit
```

Не держать отдельный tray process, IPC helper, GUI daemon или agent daemon.

## GUI boundary

`ui-contract` содержит только:

- `UiCommand`;
- `UiSnapshot`;
- `UiMode`;
- `UiVisibility`;
- `UiBackend`;
- typed UI errors/replies.

В contract запрещены типы `iced`, Wayland, layer-shell и renderer-specific state.

```text
runtime/services
      ↓ UiCommand + UiSnapshot
ui-contract
      ↓
ui-iced (future optional leaf crate)
```

Headless backend принимает logical snapshots и shutdown/hide, но явно отклоняет команды, которым нужна видимая surface.

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

На этой фазе `query` и `agent` только передают намерение в daemon state/UI contract.

## IPC path

```text
$XDG_RUNTIME_DIR/loncher/loncher.sock
```

При отсутствии `XDG_RUNTIME_DIR` не придумывать fallback молча. Для разработки разрешён явный `LONCHER_SOCKET`.

Требования:

- parent directory mode `0700`;
- socket только владельцу;
- symlink path запрещён;
- stale socket удаляется лишь после connect probe;
- active socket никогда не удаляется.

## Protocol

Transport: Unix stream socket.

Framing: `tokio_util::codec::LengthDelimitedCodec`.

Payload: versioned JSON на первом этапе.

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

## Daemon state

```rust
struct DaemonState {
    ui: UiVisibility,
    active_query: Option<String>,
    active_mode: LauncherMode,
    generation: u64,
}
```

State transitions — чистая функция с unit tests.

Инварианты:

- `Hide` идемпотентен;
- `Show` идемпотентен и может обновить query;
- `Toggle` определяется logical state, не наличием Wayland object;
- generation растёт при observable change;
- invalid empty query/prompt отклоняется до mutation.

## Runtime structure

```text
main
├── load config
├── init tracing
├── parse CLI
├── daemon path
│   ├── acquire instance ownership
│   ├── create cancellation token
│   ├── bind socket
│   ├── construct services and UiBackend
│   ├── start listener/router
│   ├── wait for signal/fatal error
│   └── cancel → join → cleanup
└── client path
    ├── connect with timeout
    ├── send one request
    ├── receive one reply
    └── exit with mapped status
```

Primitives:

- bounded `mpsc` для router;
- `oneshot` для request reply;
- `watch` для daemon snapshots;
- `CancellationToken`;
- `JoinSet` или явный task registry.

Detached tasks запрещены.

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
    Ui,
    Io,
    Task,
}

enum ProtocolError {
    UnsupportedVersion,
    InvalidFrame,
    InvalidCommand,
    RequestTooLarge,
    UiUnavailable,
    Internal,
}
```

Внутренние детали логируются с request ID; клиент получает стабильный public code без secrets/path leakage.

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

Query/prompt body по умолчанию не логируется.

## Config

На Phase 0:

- socket path override;
- request timeout;
- max frame size;
- command queue capacity;
- log filter;
- runtime role placeholder.

Приоритет:

```text
CLI > environment > XDG config > defaults
```

Secrets отсутствуют в XDG config schema. `.env` используется только локально и игнорируется Git.

## Build matrix

```bash
cargo check -p loncher --no-default-features
cargo check -p loncher --no-default-features --features sync-client
cargo check -p loncher --no-default-features --features sync-server
cargo check -p loncher --no-default-features --features desktop
cargo check -p loncher --all-features
```

Отсутствие `gui` — headless. Не вводить отрицательный feature `headless`.

## Предлагаемая разбивка commits

### 0.1 Domain protocol

- request/reply envelopes;
- command validation;
- pure state reducer;
- unit tests.

### 0.2 Paths and instance ownership

- XDG runtime path;
- secure directory/socket checks;
- stale socket probe;
- integration tests.

### 0.3 Server/client transport

- length-delimited codec;
- listener loop;
- one-request client;
- frame/version/error tests.

### 0.4 Runtime lifecycle

- command router;
- cancellation tree;
- task ownership/join;
- signals and cleanup;
- UiBackend wiring.

### 0.5 systemd and smoke tests

- user unit;
- CLI exit codes;
- end-to-end daemon/client test;
- headless feature matrix;
- documentation update.

## Tests

### Unit

- command validation;
- reducer transitions;
- protocol version;
- path validation;
- UI unavailable mapping;
- public error mapping.

### Integration

- daemon on temporary socket;
- `Show` and state reply;
- headless `Show` error;
- concurrent clients;
- malformed/oversized frame;
- second daemon rejected;
- stale socket recovered;
- shutdown removes socket;
- queue saturation deterministic.

## Out of scope

- Iced rendering and layer-shell;
- `.desktop` parsing/search;
- Niri IPC;
- SQLite;
- terminal/PTY;
- persistent sync transport/storage;
- MCP/provider APIs;
- agent memory;
- dictation.

Не добавлять stubs глубже, чем требуется для стабильных contracts и feature graph.
