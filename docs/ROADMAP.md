# Общий план `loncher`

## Цель

Собрать один Linux/Niri-first binary, который постоянно живёт как пользовательский daemon, мгновенно показывает/скрывает UI и постепенно становится desktop control plane.

Главный порядок: launcher first, AI later.

## Архитектурный инвариант

```text
daemon owns state
services own capabilities
GUI is an optional frontend adapter
sync is a first-class service
MCP exposes capabilities to the agent
agent owns sessions and memory
UI owns no durable state
```

UI напрямую использует typed Rust services. Agent использует MCP adapters над теми же services. Sync имеет собственный operation protocol и transport. Это один binary и одна реализация каждого capability.

---

## Foundation — build graph, GUI boundary и sync skeleton

Эти границы фиксируются до полноценного GUI:

- [x] `ui-contract` без зависимости от Iced/Wayland.
- [x] headless `UnavailableUiBackend`, который явно отклоняет `show/toggle`.
- [x] core build без feature `gui`.
- [x] `sync-protocol` с versioned operations, IDs и per-device cursor.
- [x] `sync-engine` с in-memory idempotency bootstrap.
- [x] feature skeleton: `gui`, `sync-client`, `sync-server`, `desktop`, `full`.
- [ ] CI проверяет headless, sync-client, sync-server, desktop и all-features.
- [ ] Реальный Iced frontend живёт отдельным leaf crate.
- [ ] Persistent sync storage/transport добавляются только после launcher MVP.

Подробности sync: [`SYNC.md`](SYNC.md).

---

## Phase 0 — daemon foundation

Подробности: [`PHASE-0-DAEMON.md`](PHASE-0-DAEMON.md).

- [ ] `systemd --user` process model.
- [ ] Один daemon process и Unix socket IPC.
- [ ] CLI-клиент в том же binary: `daemon`, `show`, `hide`, `toggle`, `query`, `agent`.
- [ ] Typed request/reply protocol, versioning и request IDs.
- [ ] Stale socket recovery и права только текущего пользователя.
- [ ] Cancellation tree, graceful shutdown, bounded queues.
- [ ] Config из XDG + environment; secrets не хранятся в config.
- [ ] `tracing`, latency spans и redaction policy.
- [ ] UI lifecycle работает через `UiBackend`, а не конкретный GUI.
- [ ] Headless build возвращает typed `UiUnavailable`, не silent no-op.

**Готово:** второй запуск отправляет команду существующему daemon, а не создаёт второй экземпляр; core собирается без GUI.

---

## Phase 1 — launcher MVP

Phase 1A implementation: [`PHASE-1A-LAUNCHER.md`](PHASE-1A-LAUNCHER.md).

- [x] application discovery and normalized desktop entries;
- [x] high-level `nucleo` fuzzy search with bounded snapshots;
- [x] framework-neutral application result contracts;
- [x] optional Iced/layer-shell frontend adapter;
- [x] shell-free launch backend with typed unsupported cases;
- [ ] daemon event routing for selection and launch actions;
- [ ] full launcher acceptance run on a Wayland/Niri session;

- [x] отдельный `ui-iced` crate с layer-shell surface на active output;
- [ ] surface создаётся/map-ится по запросу и скрывается по `Esc`/повторному hotkey;
- [x] парсинг `.desktop`: `Name`, `GenericName`, `Keywords`, `Exec`, `Icon`, actions, `NoDisplay`;
- [x] Freedesktop icon lookup и fallback;
- [x] `nucleo` fuzzy search;
- [ ] case folding, RU↔EN layout correction, typo/translit candidates и aliases;
- [ ] ranking: exact/prefix, frequency, recency, explicit alias, context;
- [ ] apps grid и плотная двухколоночная выдача;
- [ ] `Tab` autocomplete, arrows selection, `Enter` launch;
- [ ] SQLite history: query, result, launches, recency.

**Готово:** launcher ежедневно пригоден без файлов, system controls и AI.

---

## Phase 2 — Niri-native shell

- [ ] пинованная версия `niri-ipc` за собственным adapter trait;
- [ ] непрерывный Niri event stream и локальный snapshot workspaces/windows/outputs;
- [ ] workspaces в status bar: focus, occupancy, dominant app icon;
- [ ] click/scroll/context menu для workspace actions;
- [ ] поиск открытых окон и focus window;
- [ ] move focused window/result to workspace;
- [ ] launcher открывается на focused output.

Status bar резервирует компактные метрики:

- страна и публичный IP с флагом;
- latency;
- CPU, GPU, VRAM, RAM;
- RX/TX;
- battery/power;
- keyboard layout и time.

Wi-Fi/Bluetooth/display/sound здесь не дублируются.

---

## Phase 3 — files, clipboard и preview

- [ ] индекс разрешённых директорий через `ignore`;
- [ ] live updates через `notify`;
- [ ] MIME/type/metadata extraction;
- [ ] file/folder results в общей двухколоночной выдаче;
- [ ] media thumbnail вместо generic icon;
- [ ] image preview: dimensions, format, size;
- [ ] video preview: frame, duration, resolution, size;
- [ ] PDF first page, text/code, audio metadata;
- [ ] `Space` открывает широкий Quick Look-like preview;
- [ ] preview jobs cancellable; memory/disk cache;
- [ ] context menus по ПКМ: open, reveal, copy path, open with, pin, alias;
- [ ] Wayland clipboard history;
- [ ] dedup, size limits, sensitive-source policy.

---

## Phase 4 — terminal

- [ ] `!` переключает input в terminal command mode;
- [ ] adapter над `alacritty_terminal`; его типы не выходят за границу crate;
- [ ] PTY sessions, resize, scrollback, selection, copy;
- [ ] Iced renderer для terminal grid;
- [ ] command history, aliases, cwd/project context;
- [ ] one-shot commands и persistent sessions используют один backend;
- [ ] timeout/output limits и явная destructive policy.

---

## Phase 5 — system control shell

Правая панель обычного режима занимает около 20% ширины.

Компактный cluster `2×2` в едином стиле с agent tools:

- Wi-Fi status/entry tile;
- Bluetooth device/status tile;
- display/brightness tile;
- sound/output/volume tile.

Это entry points, а не огромные phone toggles. Клик открывает подробный модуль. Состояние читается по иконке, уровню и цвету; текста минимум.

Backends:

- [ ] NetworkManager через `zbus`;
- [ ] BlueZ через `bluer`;
- [ ] PipeWire/WirePlumber adapter;
- [ ] display/backlight adapter;
- [ ] MPRIS player;
- [ ] notifications;
- [ ] system metrics и AMD sysfs GPU metrics;
- [ ] public IP + local GeoIP cache.

Нижняя строка:

- MPRIS player;
- compact calendar/agenda;
- weather;
- dictation button.

---

## Phase 6 — sync production implementation

Contracts уже существуют, но реальная репликация не должна блокировать MVP.

- [ ] persistent append-only changelog;
- [ ] materialized local state;
- [ ] logical root mapping;
- [ ] HTTP transport;
- [ ] device identities и pairing;
- [ ] application-level encryption;
- [ ] per-scope permissions;
- [ ] conflict UI;
- [ ] sync hub headless role;
- [ ] optional iroh transport later.

---

## Phase 7 — MCP foundation

MCP — единая agent-facing граница, даже для встроенных capabilities.

- [ ] `rmcp` host/client/server foundation;
- [ ] builtin adapters: filesystem, console, clipboard, Niri, network, Bluetooth, display, audio, processes, metrics, sync;
- [ ] external registry: stdio и Streamable HTTP;
- [ ] EXA Search, GitHub, browser/web, DB/PostgreSQL, docs/RAG, calendar;
- [ ] reconnect/backoff, health, cancellation, output truncation;
- [ ] progressive tool discovery;
- [ ] единый policy engine и audit trail;
- [ ] access modes: Read-only, Ask, Full access;
- [ ] compact icon tool palette; docs/permissions/config/reconnect по ПКМ.

---

## Phase 8 — Hermes-like agent

### Provider layer

- [ ] OpenAI-compatible transport;
- [ ] Cerebras как основной быстрый provider с большим free tier;
- [ ] конкретные модели получаются динамически;
- [ ] OpenAI, OpenRouter, Groq и local endpoint;
- [ ] streaming, retries, fallback и фактические TTFT/tokens-per-second metrics.

### Agent loop

- [ ] sessions, messages, plans, tool calls, approvals, cancellation;
- [ ] parallel safe calls;
- [ ] context budget и tool-result compression;
- [ ] после первого `@` prompt верхний input становится session header, нижняя строка — composer;
- [ ] composer: access mode, provider/model, active tools, input, mic, send/stop;
- [ ] history — скрытый browser-like drawer.

### Memory

- [ ] working memory;
- [ ] episodic memory;
- [ ] semantic facts/preferences с provenance/scope/confidence/TTL;
- [ ] procedural skills и success checks;
- [ ] global user, project-local, machine-local и temporary scopes;
- [ ] SQLite + FTS5-first retrieval;
- [ ] compact summaries;
- [ ] memory write policy и approval;
- [ ] background consolidation только после измеримой пользы.

---

## Phase 9 — dictation

- [ ] `cpal` capture и push-to-talk;
- [ ] VAD и mic device selection;
- [ ] Groq STT backend;
- [ ] local Whisper backend;
- [ ] Groq-first, Local-only, Hedged;
- [ ] local стартует сразу, Groq уточняет финальный segment;
- [ ] не заменять текст после ручной правки;
- [ ] privacy mode запрещает network STT;
- [ ] transcript вставляется в input и не отправляется автоматически.

---

## Phase 10 — hardening

- [ ] cold/open/search/preview latency budgets и p50/p95;
- [ ] memory/cache limits и eviction;
- [ ] fake platform backends;
- [ ] corrupted media, malformed `.desktop`, hung MCP, broken streams;
- [ ] SQLite migration tests;
- [ ] filesystem scope/symlink tests;
- [ ] secret scrubbing и clipboard privacy tests;
- [ ] full audit trail для mutating agent actions;
- [ ] CI feature matrix остаётся зелёной.

---

## После MVP

### Siri-like voice frontend

- streaming STT;
- hotkey/wake frontend;
- interruption/barge-in;
- streaming TTS;
- короткий voice overlay;
- тот же AgentHost, MCP и memory.

### WebView/artifacts

- sandboxed WebView;
- agent-generated HTML/forms/tables/charts;
- controlled browser/DOM tools;
- CSP и изоляция от privileged UI;
- никаких произвольных scripts в trusted context.

## Реальный порядок исполнения

```text
0. build graph + contracts
1. daemon + IPC
2. applications + fuzzy search
3. ranking/history
4. Niri workspaces/windows
5. files + preview + clipboard
6. terminal
7. system controls + bottom bar
8. sync production
9. MCP
10. Cerebras agent + access policy
11. memory
12. dictation
13. hardening
14. voice/WebView later
```
