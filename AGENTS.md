# AGENTS.md

Правила обязательны для любых кодовых агентов, работающих в репозитории.

## 1. Цель проекта

`loncher` — один Linux/Niri-first binary, работающий как долгоживущий пользовательский daemon. UI является временной layer-shell surface и не владеет долговременным состоянием.

Приоритет разработки:

1. daemon и IPC;
2. быстрый launcher;
3. Niri/search/files/preview;
4. system controls и terminal;
5. MCP;
6. AI-agent, память и диктовка.

Не протаскивать AI в критический путь launcher-а без прямого запроса.

## 2. Архитектурные инварианты

- Один release binary; Cargo workspace допустим и желателен для границ зависимостей.
- Services владеют состоянием и capabilities.
- UI вызывает services через typed Rust API.
- Agent получает те же capabilities через MCP adapters.
- UI не пишет напрямую в SQLite, D-Bus, Niri IPC или provider API.
- Не создавать глобальный `Arc<RwLock<AppState>>` как универсальную шину.
- Для команд использовать bounded `mpsc`; для snapshots — `watch`; для потоковых событий — bounded `mpsc`/`broadcast` только при обосновании.
- Все долгоживущие задачи должны поддерживать cancellation и корректное завершение.
- Blocking I/O выполняется вне async worker threads.

## 3. Rust style

- Rust 2024, MSRV указан в workspace.
- `#![forbid(unsafe_code)]` по умолчанию. Исключения возможны только в отдельном adapter crate с документированным инвариантом безопасности.
- Не использовать `panic!`, `unwrap()` и `expect()` в production-коде.
- Ошибки внутри crates — типизированные через `thiserror`.
- Dynamic error допустим только на границе executable, если сохраняется source chain.
- Не терять ошибки через `.ok()` без комментария, почему ошибка намеренно игнорируется.
- Типы домена не должны зависеть от UI, D-Bus, MCP или конкретного provider-а.
- Newtype для идентификаторов и значений с инвариантами.
- Public API минимален; не делать поля `pub` только ради удобства тестов.
- Комментарии объясняют причину и ограничения, а не пересказывают код.
- Имена кода, commits и API — на английском. Пользовательская документация может быть на русском.

## 4. Async и daemon

- Каждая задача имеет владельца и понятный lifecycle.
- Никаких detached tasks без регистрации и shutdown policy.
- Внешние операции получают timeout.
- Очереди bounded; поведение при переполнении определено явно.
- Повторные подключения используют backoff с jitter.
- Unix socket доступен только текущему пользователю.
- Stale socket удаляется только после проверки, что daemon действительно отсутствует.

## 5. Security и privacy

- Никогда не коммитить `.env`, ключи, токены, cookies и локальные базы.
- Не логировать secrets, полный clipboard, диктовку и сырые agent prompts по умолчанию.
- Tool/MCP calls проходят policy engine до выполнения.
- `Full access` не означает произвольный shell без capability limits.
- Filesystem tools обязаны проверять scopes, canonical paths, symlinks и размер output.
- Любой внешний текст считается недоверенным и не может автоматически становиться долговременной памятью.

## 6. Agent и MCP

- MCP — единая agent-facing граница capabilities.
- Встроенные MCP tools являются adapters над теми же Rust services, что использует UI.
- Не дублировать бизнес-логику внутри MCP handlers.
- Не загружать все tool schemas в контекст: использовать progressive discovery.
- Память разделяется на working, episodic, semantic и procedural.
- Facts хранят provenance, scope, confidence и при необходимости TTL.
- FTS5-first; embeddings добавляются только после измеренного miss-rate.
- Модель не записывает устойчивые пользовательские факты без policy/approval.

## 7. UI/UX constraints

- Минимализм: текст только для имён и данных; состояние — цветом; действие — иконкой/формой.
- Внутри окна: компактный status bar, затем search/input.
- Status bar: Niri workspaces, страна/IP, latency, CPU/GPU/VRAM/RAM/network/battery/time без дублирования control center.
- Правая зона обычного режима: компактный cluster 2×2 для Wi-Fi, Bluetooth, display/brightness и sound; клик открывает детали.
- Apps — grid; остальные результаты — плотные две колонки.
- Preview заменяет generic icon для media/files.
- `Tab` — autocomplete; `Space` — wide preview; `!` — terminal; `@` — agent; web search — отдельное действие.
- Нижняя строка обычного режима: MPRIS player + calendar/weather + dictation.
- После первого agent prompt нижняя строка становится composer; история диалогов открывается browser-like drawer и по умолчанию скрыта.
- Tool palette компактна, состояния обозначены цветом; документация и permissions доступны через context menu.

## 8. Dependencies

Перед добавлением crate:

1. проверить существование, актуальность, лицензию и maintenance;
2. проверить, нельзя ли использовать уже принятую зависимость;
3. изолировать нестабильный API собственным adapter trait;
4. зафиксировать точную версию crates, которые следуют версиям внешнего проекта и не соблюдают обычный semver, например Niri IPC;
5. не включать широкие feature sets без необходимости.

## 9. Tests и quality gate

Перед завершением изменения выполнить:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Новые parser/state/policy изменения требуют unit tests. IPC, MCP и storage contracts требуют integration tests. Исправление бага должно содержать regression test, если это практически возможно.

## 10. Рабочий процесс агента

- Сначала прочитать `docs/ROADMAP.md` и план активной фазы.
- Делать небольшой вертикальный срез, а не создавать абстракции на много фаз вперёд.
- Не отмечать checkbox выполненным, пока acceptance criterion не подтверждён тестом или воспроизводимой проверкой.
- При изменении архитектурного решения обновить соответствующий документ в том же commit.
- Не менять публичные contracts скрытно.
- Не добавлять generated files, cache, local state и secrets.
- Commit message: Conventional Commits, кратко и по факту.
