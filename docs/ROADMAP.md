# Общий план `loncher`

## Цель

Собрать один Linux/Niri-first binary, который постоянно живёт как пользовательский daemon, мгновенно показывает/скрывает UI и постепенно становится desktop control plane.

Главный порядок: launcher first, AI later.

## Архитектурный инвариант

```text
daemon owns state
services own capabilities
MCP exposes capabilities to the agent
agent owns sessions and memory
UI owns no durable state
```

UI напрямую использует typed Rust services. Agent использует MCP adapters над теми же services. Это один binary и одна реализация каждого capability.

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
- [ ] UI lifecycle contract без реализации полноценного GUI.

**Готово:** второй запуск отправляет команду существующему daemon, а не создаёт второй экземпляр.

---

## Phase 1 — launcher MVP

- [ ] `iced` + layer-shell surface на focused output Niri.
- [ ] Surface создаётся/map-ится по запросу и скрывается по `Esc`/повторному hotkey.
- [ ] Парсинг `.desktop`: `Name`, `GenericName`, `Keywords`, `Exec`, `Icon`, actions, `NoDisplay`.
- [ ] Freedesktop icon lookup и fallback.
- [ ] `nucleo` fuzzy search.
- [ ] Case folding, RU↔EN layout correction, typo/translit candidates и aliases.
- [ ] Ranking: exact/prefix, frequency, recency, explicit alias, context.
- [ ] Apps grid и плотная двухколоночная выдача.
- [ ] `Tab` autocomplete, arrows selection, `Enter` launch.
- [ ] SQLite history: query, result, launches, recency.

**Готово:** launcher ежедневно пригоден без файлов, system controls и AI.

---

## Phase 2 — Niri-native shell

- [ ] Пинованная версия `niri-ipc` за собственным adapter trait.
- [ ] Непрерывный Niri event stream и локальный snapshot workspaces/windows/outputs.
- [ ] Workspaces в status bar: focus, occupancy, dominant app icon.
- [ ] Клик/scroll/context menu для workspace actions.
- [ ] Поиск открытых окон и focus window.
- [ ] Move focused window/result to workspace.
- [ ] Launcher открывается на focused output.

Status bar также резервирует компактные метрики:

- [ ] страна и публичный IP с флагом;
- [ ] latency;
- [ ] CPU, GPU, VRAM, RAM;
- [ ] RX/TX;
- [ ] battery/power;
- [ ] keyboard layout и time.

Не дублировать там Wi-Fi/Bluetooth/display/sound control tiles.

---

## Phase 3 — files, clipboard и preview

- [ ] Индекс разрешённых директорий через `ignore`.
- [ ] Live updates через `notify`.
- [ ] MIME/type/metadata extraction.
- [ ] File/folder results в общей двухколоночной выдаче.
- [ ] Media thumbnail вместо generic icon.
- [ ] Image preview: dimensions, format, size.
- [ ] Video preview: frame, duration, resolution, size.
- [ ] PDF first page, text/code, audio metadata.
- [ ] `Space` открывает широкий Quick Look-like preview.
- [ ] Preview jobs cancellable; memory/disk cache.
- [ ] Context menus по ПКМ: open, reveal, copy path, open with, pin, alias.
- [ ] Wayland clipboard history: text, URL, code, image, files.
- [ ] Dedup, size limits, sensitive-source policy.

---

## Phase 4 — terminal

- [ ] `!` переключает input в terminal command mode.
- [ ] Adapter над `alacritty_terminal`; его типы не выходят за границу adapter-а.
- [ ] PTY sessions, resize, scrollback, selection, copy.
- [ ] Iced renderer для terminal grid.
- [ ] Command history, aliases, cwd/project context.
- [ ] One-shot commands и persistent sessions используют один backend.
- [ ] Timeout/output limits и явная destructive policy.

---

## Phase 5 — system control shell

Правая панель обычного режима занимает около 20% ширины.

Компактный cluster `2×2` в едином стиле с agent tools:

- [ ] Wi-Fi status/entry tile;
- [ ] Bluetooth device/status tile;
- [ ] display/brightness tile;
- [ ] sound/output/volume tile.

Это entry points, а не огромные phone toggles. Клик открывает подробный модуль. Состояние читается по иконке, уровню и цвету; текста минимум.

Backends:

- [ ] NetworkManager через `zbus`.
- [ ] BlueZ через `bluer`.
- [ ] PipeWire/WirePlumber adapter.
- [ ] display/backlight adapter.
- [ ] MPRIS player.
- [ ] notifications.
- [ ] system metrics и AMD sysfs GPU metrics.
- [ ] public IP + local GeoIP cache.

Нижняя строка обычного режима:

- [ ] MPRIS player;
- [ ] progress/playback controls;
- [ ] compact calendar/agenda;
- [ ] weather;
- [ ] dictation button.

---

## Phase 6 — MCP foundation

MCP — единая agent-facing граница, даже для встроенных capabilities.

- [ ] `rmcp` host/client/server foundation.
- [ ] Builtin MCP server adapters: filesystem, console, clipboard, Niri, network, Bluetooth, display, audio, processes, metrics.
- [ ] External MCP registry: stdio и Streamable HTTP.
- [ ] EXA Search, GitHub, browser/web, DB/PostgreSQL, docs/RAG, calendar.
- [ ] Reconnect/backoff, health, cancellation, output truncation.
- [ ] Progressive tool discovery вместо загрузки всех schemas.
- [ ] Единый policy engine и audit trail.
- [ ] Access modes: Read-only, Ask, Full access.
- [ ] Tool palette: компактные иконки, цветные состояния, click toggle, context menu для docs/permissions/config/reconnect.

---

## Phase 7 — Hermes-like agent

### Provider layer

- [ ] OpenAI-compatible transport.
- [ ] Cerebras как основной быстрый provider с большим free tier.
- [ ] Конкретные модели получаются динамически и не подменяются именем provider-а.
- [ ] OpenAI, OpenRouter, Groq и local endpoint как дополнительные backends.
- [ ] Streaming, retries, fallback и фактические TTFT/tokens-per-second metrics.

### Agent loop

- [ ] Sessions, messages, plans, tool calls, approvals, cancellation.
- [ ] Parallel safe calls.
- [ ] Context budget и tool-result compression.
- [ ] После первого `@` prompt верхний input становится session header, нижняя строка — composer.
- [ ] Composer: access mode, provider/model, active tools, input, mic, send/stop.
- [ ] History — скрытый browser-like drawer, не постоянная колонка.

### Memory

- [ ] Working memory: текущий plan, context, pending calls.
- [ ] Episodic memory: задачи, действия, ошибки, результат, corrections.
- [ ] Semantic memory: устойчивые facts/preferences с provenance/scope/confidence/TTL.
- [ ] Procedural memory: reusable skills и success checks.
- [ ] Раздельные global user, project-local, machine-local и temporary scopes.
- [ ] SQLite + FTS5-first retrieval.
- [ ] Compact summaries вместо сырых старых диалогов.
- [ ] Memory write policy и approval для устойчивых пользовательских фактов.
- [ ] Background consolidation после появления измеримой пользы.

---

## Phase 8 — dictation

- [ ] `cpal` capture и push-to-talk.
- [ ] VAD и mic device selection.
- [ ] Groq STT backend.
- [ ] Local Whisper backend.
- [ ] Policies: Groq-first, Local-only, Hedged.
- [ ] В Hedged режиме local стартует сразу, Groq уточняет финальный segment.
- [ ] Не заменять текст после ручной правки.
- [ ] Privacy mode запрещает network STT.
- [ ] Transcript вставляется в input и не отправляется автоматически.

---

## Phase 9 — hardening

- [ ] Cold/open/search/preview latency budgets и p50/p95 measurements.
- [ ] Memory/cache limits и eviction.
- [ ] Fake platform backends для integration tests.
- [ ] Corrupted media, malformed `.desktop`, hung MCP, broken provider streams.
- [ ] SQLite migration tests.
- [ ] Filesystem scope/symlink tests.
- [ ] Secret scrubbing и clipboard privacy tests.
- [ ] Full audit trail для mutating agent actions.

---

## После MVP

### Siri-like voice frontend

- streaming STT;
- hotkey/wake frontend;
- interruption/barge-in;
- streaming TTS;
- короткий voice overlay;
- тот же AgentHost, MCP и memory, без второго агента.

### WebView/artifacts

- sandboxed WebView;
- agent-generated HTML/forms/tables/charts;
- controlled browser/DOM tools;
- CSP и изоляция от privileged UI;
- никаких произвольных HTML scripts в trusted context.

### Возможные frontends

- terminal sessions;
- scheduled jobs;
- Telegram/other gateways;
- rich approval/diff surfaces.

---

## Реальный порядок исполнения

```text
1. daemon + IPC
2. applications + fuzzy search
3. ranking/history
4. Niri workspaces/windows
5. files + preview + clipboard
6. terminal
7. system controls + bottom bar
8. MCP
9. Cerebras agent + access policy
10. memory
11. dictation
12. hardening
13. voice/WebView later
```
