# AGENTS.md

Правила обязательны для любых кодовых агентов, работающих в репозитории.

## 1. Цель проекта

`loncher` — один Linux/Niri-first binary, работающий как долгоживущий пользовательский daemon. GUI является опциональной временной layer-shell surface и не владеет долговременным состоянием.

Приоритет:

1. daemon и IPC;
2. быстрый launcher;
3. Niri/search/files/preview;
4. system controls и terminal;
5. sync production;
6. MCP;
7. AI-agent, память и диктовка.

Не протаскивать AI в критический путь launcher-а без прямого запроса.

## 2. Архитектурные инварианты

- Один release binary; Cargo workspace используется для границ зависимостей.
- Daemon владеет durable/runtime state.
- Services владеют capabilities.
- GUI — optional frontend adapter.
- Core обязан собираться без GUI dependencies.
- UI вызывает services через typed Rust API.
- Agent получает те же capabilities через MCP adapters.
- Sync — отдельный service со своим operation protocol; MCP не является sync transport.
- UI не пишет напрямую в SQLite, D-Bus, Niri IPC, sync transport или provider API.
- Не создавать глобальный `Arc<RwLock<AppState>>`.
- Commands: bounded `mpsc`; snapshots: `watch`; streams: bounded channels с обоснованием.
- Все долгоживущие задачи поддерживают cancellation и join.
- Blocking I/O выполняется вне async worker threads.

## 3. GUI boundary

- `ui-contract` не зависит от Iced, Wayland или layer-shell.
- Renderer/framework types не выходят из будущего `ui-iced` crate.
- Не создавать абстракцию над widgets (`Button`, `Row`, `RenderTree`). Абстрагировать commands, snapshots и events.
- Headless backend не врёт об успехе: видимые UI actions возвращают typed `UnavailableInBuild`.
- Iced frontend должен быть leaf crate.
- UI state, который нужен после закрытия surface, принадлежит daemon/service, не widget tree.

## 4. Features и build profiles

Cargo features аддитивны.

- отсутствие `gui` означает headless;
- отрицательный feature `headless` запрещён;
- runtime roles не кодируются mutually-exclusive features;
- optional dependency должна быть реально optional и не протекать в core;
- новый feature добавляется в CI matrix;
- `--all-features` и `--no-default-features` обязаны собираться.

Текущие profiles:

```text
core:          --no-default-features
sync client:   --no-default-features --features sync-client
sync server:   --no-default-features --features sync-server
desktop:       --no-default-features --features desktop
full:          --all-features
```

## 5. Rust style

- Rust 2024, MSRV указан в workspace.
- `#![forbid(unsafe_code)]` по умолчанию.
- Исключения unsafe возможны только в отдельном adapter crate с документированным safety invariant.
- Не использовать `panic!`, `unwrap()` и `expect()` в production-коде.
- В tests `expect()` допустим для fixture setup.
- Ошибки внутри crates типизированы через `thiserror`.
- Dynamic error допустим на executable boundary с сохранением source chain.
- Не терять ошибки через `.ok()` или `let _ =` без объяснения.
- Domain types не зависят от UI, D-Bus, MCP, sync transport или provider.
- Newtype для идентификаторов и значений с инвариантами.
- Public API минимален.
- Комментарии объясняют причину и ограничения.
- Code, commits и API — на английском; user docs могут быть на русском.

## 6. Async и daemon

- Каждая задача имеет владельца и lifecycle.
- Detached tasks запрещены.
- Внешние операции получают timeout.
- Очереди bounded; overflow policy явная.
- Reconnect использует backoff с jitter.
- Unix socket доступен только текущему пользователю.
- Stale socket удаляется лишь после проверки отсутствия daemon.

## 7. Sync

- Не синхронизировать SQLite-файл целиком.
- Operations versioned и идемпотентны.
- Device sequence монотонен; cursor per device.
- Payload opaque и позже шифруется application-level.
- Merge policy выбирается по entity type.
- Secrets, private keys, raw mic audio и machine-local state не синхронизируются.
- Remote actions — отдельное capability и default deny.
- Persistent transport/storage не добавлять раньше рабочего launcher MVP, кроме необходимого contract skeleton.
- Любое изменение wire types требует schema/version review и compatibility tests.

## 8. Security и privacy

- Никогда не коммитить `.env`, keys, tokens, cookies и local DB.
- Не логировать secrets, полный clipboard, диктовку и raw agent prompts по умолчанию.
- Tool/MCP calls проходят policy engine.
- `Full access` не означает arbitrary shell без capability limits.
- Filesystem tools проверяют scopes, canonical paths, symlinks и output size.
- Внешний текст недоверен и не становится памятью автоматически.
- Sync hub не должен требовать plaintext пользовательских данных.

## 9. Agent и MCP

- MCP — единая agent-facing граница capabilities.
- Builtin tools — adapters над теми же Rust services, что использует UI.
- Не дублировать бизнес-логику в MCP handlers.
- Использовать progressive discovery.
- Память: working, episodic, semantic, procedural.
- Facts хранят provenance, scope, confidence и TTL при необходимости.
- FTS5-first; embeddings только после измеренного miss-rate.
- Модель не записывает устойчивые user facts без policy/approval.

## 10. UI/UX constraints

- Минимализм: текст только для имён и данных; состояние — цветом; действие — иконкой/формой.
- Внутри окна: compact status bar, затем search/input.
- Status bar: Niri workspaces, страна/IP, latency, CPU/GPU/VRAM/RAM/network/battery/time без дублирования control center.
- Правая зона: compact 2×2 Wi-Fi/Bluetooth/display/sound; клик открывает details.
- Apps — grid; остальные результаты — плотные две колонки.
- Preview заменяет generic icon для media/files.
- `Tab` — autocomplete; `Space` — wide preview; `!` — terminal; `@` — agent.
- Нижняя строка: MPRIS + calendar/weather + dictation.
- После первого agent prompt нижняя строка становится composer.
- History — browser-like drawer и по умолчанию скрыта.
- Tool palette compact; state цветом; docs/permissions по context menu.

## 11. Dependencies

Перед добавлением crate:

1. проверить актуальность, лицензию и maintenance;
2. проверить переиспользование принятой зависимости;
3. изолировать нестабильный API adapter trait;
4. pin version для проектов без обычного semver, например Niri IPC;
5. не включать широкие features без необходимости.

## 12. Tests и quality gate

Перед завершением изменения:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check -p loncher --no-default-features
```

Новые parser/state/policy/sync изменения требуют unit tests. IPC, MCP, storage и sync contracts требуют integration tests. Bugfix содержит regression test, когда практически возможно.

## 13. Рабочий процесс агента

- Сначала прочитать `docs/ROADMAP.md` и план активной фазы.
- Для sync прочитать `docs/SYNC.md`.
- Делать небольшой вертикальный срез.
- Не отмечать checkbox выполненным без теста или воспроизводимой проверки.
- Архитектурное решение и docs меняются в одном commit.
- Не менять public contracts скрытно.
- Не добавлять generated files, cache, local state и secrets.
- Commit message: Conventional Commits, кратко и по факту.
