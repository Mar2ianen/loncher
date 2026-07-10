# Sync architecture

## Цель

Синхронизировать настройки, aliases, selected agent memory и другие переносимые сущности между desktop, headless node и server/hub, не превращая sync в remote shell и не связывая его с GUI.

Sync закладывается сейчас на уровне contracts и feature graph. Persistent storage, transport и pairing реализуются после рабочего launcher MVP.

## Инварианты

```text
sync is not GUI
sync is not MCP transport
server must not need plaintext
operations are idempotent
offline-first is mandatory
device-local state stays local
```

- UI использует `SyncService` через typed Rust API.
- Агент позже получает `sync.status`, `sync.list_devices`, `sync.trigger` и conflict actions через MCP adapter.
- Сама репликация через MCP не идёт.
- Не синхронизировать SQLite-файл целиком.
- Секреты и device private keys никогда не синхронизируются.
- Remote actions отделены от state sync и по умолчанию запрещены.

## Crates

```text
crates/sync-protocol
    versioned wire/domain types, operation IDs, cursors, payload envelope

crates/sync-engine
    validation, idempotency, merge dispatch, changelog/materialization

future:
crates/sync-storage
crates/sync-http
crates/sync-crypto
```

Текущий `InMemorySyncEngine` нужен для фиксации семантики и тестов. Это не production storage.

## Что синхронизировать

### По умолчанию

- aliases;
- pinned applications/actions;
- launcher preferences;
- theme/layout;
- MCP server definitions без secrets;
- procedural skills;
- approved semantic memory;
- conversation metadata;
- explicitly selected sessions.

### Только opt-in

- launch/query history;
- full agent conversations;
- clipboard history;
- files/artifacts.

### Только локально

- Unix sockets и process IDs;
- Niri workspace/window IDs;
- current UI selection;
- audio/Bluetooth/display device state;
- absolute machine-specific paths;
- preview/thumbnail caches;
- API keys, cookies и tokens;
- device private keys;
- temporary working memory;
- raw microphone audio.

Machine-specific paths представляются logical roots:

```text
project:nedobot/src/main.rs
```

Каждое устройство хранит собственное отображение logical root в локальный путь.

## Operation model

```text
SyncOperation
├── schema_version
├── operation_id
├── device_id
├── device_sequence
├── entity key
└── encrypted Put/Delete payload
```

Требования:

- `(device_id, device_sequence)` монотонны;
- `operation_id` глобально идемпотентен;
- cursor хранится per device;
- повторная доставка безопасна;
- delete представлен tombstone;
- payload opaque для hub;
- schema version проверяется до записи.

## Merge policy

Политика выбирается по типу сущности, а не одна на всю базу:

- config field — last writer wins;
- aliases — per-key revisions;
- conversation messages — append-only;
- bounded history — append-only + deterministic pruning;
- skills — manual conflict или forked revisions;
- semantic facts — explicit entity revision + provenance.

CRDT/Automerge не использовать глобально до появления реального concurrent-editing use case.

## Transport

Первая production-реализация:

```text
reqwest client ↔ axum sync hub
rustls
device identity
application-level E2E encryption
```

Позже возможен `iroh`/QUIC transport за тем же trait.

## Pairing и trust

Каждая установка создаёт:

- `DeviceId`;
- signing key;
- encryption key;
- human-readable device name;
- capability set.

Pairing использует одноразовый token/QR и явное подтверждение. Permissions scoped отдельно:

- settings;
- memory;
- sessions;
- clipboard;
- remote actions.

`remote actions` всегда `false` по умолчанию.

## Build profiles

```bash
cargo build -p loncher --no-default-features
cargo build -p loncher --no-default-features --features sync-client
cargo build -p loncher --no-default-features --features sync-server
```

Feature flags управляют dependency graph. Runtime role (`desktop`, `node`, `sync-hub`) позже выбирается config/CLI и не кодируется взаимоисключающими Cargo features.
