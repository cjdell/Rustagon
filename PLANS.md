# PLANS — Rustagon Professionalisation Pass

The goal of this document: turn Rustagon into a truly professional piece of
software by **removing all tech debt and awkwardness first**, then layering on
carefully chosen refinements. It replaces the previous SSH-focused plan
(superseded — see "Already addressed" below).

## How this document works

- Work is organised into **ordered blocks** (dependencies run top to bottom).
- Each block is written as a **self-contained instruction for a single agent
  session** (250K token context budget). An agent given one block should be
  able to go away, do the work, verify it, and report back — without needing
  to re-explore the whole codebase.
- The agent's final report for every block must include: files changed, the
  exact verification commands run and their outcomes, any design decisions
  taken (and why), and anything left unfinished with a clear reason.
- Each block ends with **one commit** (`Block N — <title>`), working tree
  clean, unless the block says otherwise. If a block runs out of context, stop
  at the marked split point, commit what exists, and report so the remainder
  can be resumed in a fresh session.
- Every block that changes structure, APIs, or crate governance **must update
  `AGENTS.md`** in the same commit so it never drifts from reality.
- Update the status table at the end of this file when a block lands.

### Rules that apply to every block (do not repeat them in the block text)

1. **All builds inside `nix develop`.** Never run `cargo`/`just` from a bare
   shell — wrong toolchains and the `critical-section` feature breakage are
   the classic failure. `nix develop --command bash -c "just ..."`.
2. **Formatting:** after editing Rust, run
   `rustfmt --edition 2021 --config skip_children=true <file>` per edited
   file. Never `cargo fmt` the workspace. Web files: `cd web && deno fmt
   <file>`. Match the existing style — no comments unless the code genuinely
   needs them, no emojis in code or logs.
3. **Crate governance is law** (`AGENTS.md`): `app/` stays `no_std`, never
   depends on embassy-net/esp-hal/ureq/reqwless/hardware crates; `firmware/`
   never contains menu/app domain logic; `desktop/` is I/O-centric with no
   menu logic or WASM awareness; `libs/wasm_protocol` is wire types only.
4. **WASM size discipline** (`AGENTS.md` "Keeping WASM apps small"): no
   std-pulling deps in `sdk/`, serde without `std`, `fast_sin/fast_cos`, keep
   `-C strip=symbols`. After any SDK change, `just build_sdk` and verify every
   app still runs (`just run_desktop_app <name>`, expect `WASM: Program
   complete`, no panic).
5. **Build profiles:** firmware and SDK are built `--profile release-lto`;
   host crates plain `-r`. Use the `just` recipes, not bare cargo.
6. **Preferred patterns** (from AGENTS.md): `CriticalSectionRawMutex` for any
   shared state; `EventQueue<T, N>` for events, `WatchedValue<T>` for state;
   never `loop { check_shared_vec(); Timer::after(..).await }`; futures must
   always be `.await`ed; `StackSignal` (atomic swap + signal) for
   single-consumer events, never a shared channel with multiple receivers.
7. **Verification standard:** every block ends with the listed checks green —
   at minimum `cd desktop && cargo build`, `cd app && cargo build --features
   wasm-runtime`, `cd firmware && cargo build -r --lib`, plus the block's own
   tests. Anything requiring hardware is called out explicitly; if you have
   no hardware, say so in the report instead of guessing.

## Already addressed — do not redo

Before any block starts, note what has **already landed** (so agents don't
"fix" working code or duplicate done work):

- `app` crate **feature flags** exist: `std`, `tokio`, `embassy`,
  `http-server`, `extern-alloc`, `web-bundle`, `wasm-runtime` (AGENTS "Medium
  Priority 1" — done).
- **`AppError` + `ctx.notify()`** toasts and the app logging convention
  (`app/src/apps/common.rs`) — old plan 9a/9b done.
- **`MenuApp::tick()`** cadence (`APP_TICK_MS = 50` in `app/src/menu/mod.rs`)
  and **`on_stop`/`on_shown` lifecycle hooks** — old plan 1b/1c done.
- **Clock/timeout helpers**: `now()`, `sleep()`, `select_timeout()`,
  `with_timeout()`, `retry()` in `app/src/utils/mod.rs` — old plan 2b done.
- **SSH app + engine**: `app/src/apps/ssh.rs`, `app/src/ssh/` (no_std puressh
  state machine + terminal), with e2e tests (`cargo test -p app`).
- **External allocation helpers**: `app/src/alloc_ext.rs` (external box/vec).
- **TCP platform capability**: `Platform::tcp_client()` →
  `app/src/platform/tcp.rs`; firmware pump task in `firmware/src/platform/tcp.rs`;
  desktop impl in `desktop/src/platform/tcp.rs`.
- **`Platform::entropy()` and `Platform::firmware_version()`** exist.
- **HTTP server routes moved into `app`** (`app/src/http/`, picoserve behind
  the `http-server` feature); firmware/desktop keep thin server tasks.
- **`software_reset()` via Platform** — no direct `esp_hal::system` calls
  outside the firmware platform impl (AGENTS "Medium 3" — done).
- **LED dedup** (`LedState` single source), **wasmi v1.1.0 with PSRAM
  support**, **embassy/HAL ecosystem upgrade** — done in recent commits.
- **`#[serde(default)]`** on `device_name` and `ap_password` in
  `DeviceConfig`; `app/src/keys.rs` shift/symbol mapping centralised.
- **Desktop mirrors firmware task structure** (wasm runner, http task, ipc
  forwarders in `desktop/src/tasks/`).

## Remaining debt inventory (what the blocks fix)

| # | Debt | Where | Block |
|---|------|-------|-------|
| 1 | No CI, no test/lint/check recipes, unknown warnings | repo root, `justfile` | 1 |
| 2 | `WatchedValue`/`EventQueue` live in firmware; desktop uses bespoke statics | `firmware/src/utils/`, `desktop/src/platform/` | 2 |
| 3 | Firmware HTTP hardcodes GET, ignores method/body; duplicate `HttpEvent` type; `Done` sent on error | `firmware/src/utils/http.rs` | 3 |
| 4 | OTA parks app core and never unparks on failure; `CpuControl::steal()` boilerplate ×5; `Ota::free()` no-op; power manager polls on demand with chatty logs; `println!`/emoji in firmware | `firmware/src/platform/hardware.rs`, `firmware/src/utils/{cpu_guard,local_fs,ota}.rs`, `firmware/src/platform/power.rs` | 4 |
| 5 | `Box::leak` per SSH session + per TCP connection (PSRAM leak) | `app/src/apps/ssh.rs`, `firmware/src/platform/tcp.rs`, `desktop/` | 5 |
| 6 | Apps are callback-driven; menu has 3 duplicated handlers with nested `Either` selects; no background-task spawn for apps | `app/src/menu/mod.rs`, `app/src/apps/common.rs` | 6 |
| 7 | No `MockPlatform`, no headless app driver, no golden screens | `app/` | 7 |
| 8 | Editor/SSH hand-roll text fields; terminal is SSH-specific | `app/src/apps/editor.rs`, `app/src/ssh/terminal.rs` | 8 |
| 9 | No app→app navigation with results; no per-app persistence | `app/src/menu/`, `app/src/apps/files.rs` | 9 |
| 10 | Desktop hexpansion/wifi are stubs; icons unverified; no key repeat | `desktop/src/platform/common.rs` | 10 |
| 11 | Legacy `libs/emulator` + `just emulate_wasm`; games untested; no WASM size audit in CI | `libs/emulator/`, `sdk/wasm/` | 11 |
| 12 | Keyboard hexpansion polls at 20 ms instead of using HS_H interrupt | `firmware/src/platform/drivers/tca8418.rs` | 12 |
| 13 | `AGENTS.md` stale; nightly features in `app`; `anyhow` leakage; leftover TODOs | misc | 13 |

## Status table

| Block | Title | Depends on | Status |
|-------|-------|------------|--------|
| 1 | CI, lint & build hygiene foundation | — | done |
| 2 | Move `WatchedValue`/`EventQueue` into `app`; unify desktop managers | 1 | done |
| 3 | HTTP client correctness + `HttpEvent` dedup | 1 | done |
| 4 | Firmware platform hardening: OTA/CPU parking, power loop, logging | 2 | done |
| 5 | TCP session lifecycle — kill the `Box::leak`s | 2 | done |
| 6 | Menu/app model: apps own their loop (`MenuApp::run` + spawner) | 5 | not started |
| 7 | `MockPlatform` + headless app driver + golden screens | 6 | not started |
| 8 | Shared widget toolkit (`app/src/ui/`) | 6 | not started |
| 9 | App navigation with results + per-app storage | 6 (8 recommended) | not started |
| 10 | Desktop platform parity (hexpansion/wifi sim, icons, key repeat) | 2, 6 | not started |
| 11 | WASM SDK parity & tooling (games, emulator removal, size audit) | 3 | not started |
| 12 | Interrupt-driven keyboard (requires hardware) | 4 | not started |
| 13 | Final polish & documentation sync | all | not started |

---

## Block 1 — CI, lint & build hygiene foundation

### Goal

Give every later block a safety net: repeatable `just` recipes for check/test/
lint, a GitHub Actions pipeline that runs them, a warning-free host build, and
web import checks wired into the build.

### Background

- The repo is on GitHub (`github.com/cjdell/Rustagon`) but has **no CI**
  (no `.github/workflows/`). AGENTS.md documents manual checks only.
- The `justfile` has no `check`/`test`/`lint` recipes. Tests exist in `app`
  (`app/src/keys.rs`, `app/src/ssh/{mod,terminal,tests}.rs`) but nothing runs
  them automatically. `app` has a host-side ssh e2e test using a dev-dep
  `puressh` server (`app/Cargo.toml` dev-dependencies).
- `web/tools/check_imports.ts` enforces `@lib` import conventions but is **not
  wired into `deno task build`** (AGENTS.md calls this out).
- AGENTS.md "Remaining Work — Medium 2" (warnings cleanup) is not done. You
  are expected to find and fix warnings as part of making lint green.
- Firmware compiles without the Xtensa linker via `cargo build -r --lib`; the
  nix flake does provide the full Xtensa toolchain, but linking the full
  firmware binary in CI may be slow — you decide and document.

### Work items

1. **Add `just` recipes** to the root `justfile` (keep the existing recipes
   untouched):
   - `just check` — builds everything that can build without hardware:
     - `cd desktop && cargo build`
     - `cd app && cargo build --features wasm-runtime`
     - `cd app && cargo build --features std` (if it differs)
     - `cd firmware && cargo build -r --lib`
     - `cd web && deno task check` (or whatever typecheck command exists —
       inspect `web/deno.json`; run `deno task build` if that is the only
       check)
     - manifest/uploader tools: `cargo build -p manifest-tool -p uploader`
   - `just test` — `cargo test -p app` (host, includes ssh e2e), plus any
     desktop/tools tests.
   - `just lint` — `cargo clippy` on `app` (all default features),
     `desktop`, `firmware` `--lib`, with `-D warnings` for the host crates.
     Web: `cd web && deno task check-imports` (add a `deno task lint` alias
     if useful).
2. **Fix every warning** the above surfaces across `app/`, `desktop/`,
   `firmware/` (lib target) so lint is clean. Unused imports left over from
   earlier refactors are the expected bulk. Do not suppress with blanket
   `#[allow]` — fix the code; if a warning is genuinely unfixable, keep the
   `#[allow]` local to the item and note it in your report.
3. **GitHub Actions workflow** (`.github/workflows/ci.yml`): ubuntu-latest,
   install Nix (e.g. `cachix/install-nix-action`), then
   `nix develop --command bash -c "just check && just test && just lint"`.
   Cache the cargo target dir and nix store sensibly. Decide whether to
   include the full firmware binary link (`just build_firmware`) — if the
   flake's Xtensa toolchain works on the Linux runner, include it (it is the
   only thing that catches link-time issues); if it is too slow/flaky,
   restrict CI to `--lib` and say so in AGENTS.md. `firmware/.env` is
   committed (single line, `FIRMWARE_VERSION=...`), so no secrets needed.
4. **Wire web import checks into the build**: make `deno task build` (or an
   equivalent pre-build step) run `check-imports` so CI covers it. Update the
   AGENTS.md web section accordingly.
5. **`DeviceConfig` serde audit** (AGENTS "Low 3"): every field added after
   the initial release needs `#[serde(default)]`. `device_name` and
   `ap_password` have it; verify `known_wifi_networks` and anything else.
   Add a small deserialisation test in `app/src/types.rs` (or a
   `#[cfg(test)]` module near it) proving a **minimal legacy JSON** (only
   original fields) deserialises into `DeviceConfig`.
6. **Update AGENTS.md**: document the new recipes and CI in the Build
   section; strike the items this block resolves from "Remaining Work".

### Acceptance

- `just check`, `just test`, `just lint` all pass locally inside
  `nix develop`.
- CI workflow is green on push (you may not be able to verify the remote
  action; at minimum ensure the exact commands succeed locally on Linux
  semantics — say so in the report).
- Zero warnings in host crates under clippy `-D warnings` (firmware lib
  target clean as far as the esp fork's clippy allows; report exceptions).

### Report

List: recipes added, workflow YAML summary, warnings fixed (count + notable
ones), the CI firmware-link decision, serde test name.

---

## Block 2 — Move `WatchedValue`/`EventQueue` into `app`; unify desktop managers

### Goal

Make the two sync primitives (`WatchedValue<T>` for state, `EventQueue<T,N>`
for events) first-class citizens of the platform-agnostic `app` crate, and
make the desktop managers structurally mirror the firmware managers
(manager-owned queues instead of module-level statics). This is AGENTS.md
"High Priority 2" and a prerequisite for Blocks 4, 5 and 10.

### Background

- `firmware/src/utils/watched_value.rs` and `event_queue.rs` are
  platform-agnostic (embassy-sync only). Firmware managers already use them:
  `firmware/src/platform/wifi.rs` (`WatchedValue<WifiStatus>`),
  `input.rs` (`EventQueue<HexButton, N>` via `ButtonEventQueue`),
  `system.rs` (`EventQueue<SystemMessage, N>`), `hexpansion.rs`
  (`EventQueue<HexpansionEvent, N>`), and `drivers/mod.rs`
  (`DeviceEventQueue`, `DEVICE_EVENT_QUEUE_DEPTH` type aliases).
- The desktop side hand-rolls equivalents with module-level statics:
  `desktop/src/platform/common.rs` has `static SYSTEM_SIGNAL`
  (embassy `Signal`) and `static DEVICE_EVENT_CHANNEL`; the desktop input
  manager (`desktop/src/platform/input.rs`) has its own static channel behind
  `DesktopInputManager::push_button`. The desktop wifi/hexpansion managers
  return hardcoded stubs and `pending()` futures.
- `app` may depend on embassy-sync/embassy-futures/embassy-time — allowed per
  governance. `app/src/utils/` already exists (`mod.rs` with time helpers) —
  the natural home is `app/src/utils/sync.rs`.

### Work items

1. **Move** `firmware/src/utils/watched_value.rs` and `event_queue.rs` to
   `app/src/utils/sync.rs` (or `watched_value.rs` + `event_queue.rs` under
   `app/src/utils/` — your call, keep docs). Re-export as
   `app::utils::{WatchedValue, EventQueue}`. No API changes except what
   unification forces.
2. **Update all firmware imports** to `app::utils::*` (wifi, input, system,
   hexpansion, drivers). Delete the old files. The `ButtonEventQueue` /
   `DeviceEventQueue` type aliases stay in firmware where they are used, but
   now alias `app::utils::EventQueue`.
3. **Desktop managers — mirror firmware structure:**
   - `DesktopInputManager`: own an `EventQueue<HexButton, N>` (same depth as
     firmware); `push_button` pushes to it; `next_button` consumes it. Remove
     the static.
   - `DesktopSystemManager`: replace `SYSTEM_SIGNAL` static with a
     manager-owned `EventQueue<SystemMessage, N>`; keep `push_message` as the
     minifb-thread entry point (make the manager own a cloneable handle so a
     non-`'static` reference is not needed — see AGENTS "Event Delivery
     Pattern").
   - `DesktopHexpansionManager`: replace `DEVICE_EVENT_CHANNEL` static with an
     owned `EventQueue<DeviceEvent, 32>`, **and add** an
     `EventQueue<HexpansionEvent, N>` for real events (still empty until
     Block 10 — `next_event` may keep returning `pending()` for now, but the
     queue and injection point must exist).
   - `DesktopWifiManager`: give it a `WatchedValue<WifiStatus>` instance
     instead of hardcoding `WifiStatus::Offline`; `wait_for_status_change`
     uses the watched value's wait mechanism. `scan()` still returns an empty
     vec for now.
4. **Check every use site for behaviour changes**: `try_next_event` /
   `try_receive` semantics must be preserved so the menu's non-blocking
   drains still work. Desktop `main.rs` calls into `DesktopInputManager::push_button`,
   `DesktopSystemManager::push_message`, `DesktopHexpansionManager::push_device_event` —
   keep those entry points working.
5. **Update AGENTS.md**: move `WatchedValue`/`EventQueue` out of
   "Remaining Work", update the Related Files section, and rewrite the
   "Event Delivery Pattern" section to point at `app/src/utils/`.

### Acceptance

- `cd firmware && cargo build -r --lib` clean (no more
  `firmware/src/utils/{watched_value,event_queue}.rs`).
- Desktop builds; menu navigation works via arrows/Enter
  (`just run_desktop`); a WASM app still launches and stops
  (`just run_desktop_app bare`).
- `cargo test -p app` still green.
- Grep confirms no module-level channel/signal statics left in
  `desktop/src/platform/` except where a block-10 simulation will own them.

### Report

Files changed; any API tweaks to `WatchedValue`/`EventQueue` needed during
the move; desktop run-through results.

---

## Block 3 — HTTP client correctness + `HttpEvent` dedup

### Goal

Make the firmware HTTP client honour `HttpRequest.method` and
`HttpRequest.body` (it currently hardcodes GET), delete the duplicated
`HttpEvent` type, fix the "Done sent on error" bug, and bring the desktop
client up to the same contract (it currently drops `req.headers`). AGENTS.md
"High Priority 1".

### Background

- `firmware/src/utils/http.rs::perform_http_request_streaming` calls
  `client.request(Method::GET, &http_request.url)` — `http_request.method`
  and `http_request.body` are **ignored** (line ~128).
- `firmware/src/utils/http.rs` defines its own `HttpEvent { Meta, Chunk,
  Done }` — a duplicate of `app::protocol::HttpEvent` (which also has an
  `Error` variant). `firmware/src/platform/http.rs` imports
  `HttpEvent` from `app::protocol`, so two enums coexist.
- `perform_http_request_channel` (same file) sends `HttpEvent::Done`
  unconditionally after the streaming call — even when the request errored.
- Desktop (`desktop/src/platform/mod.rs::DesktopHttpClient`) handles all four
  methods but **ignores `req.headers`**.
- `app::protocol::HttpRequest` (`wasm_protocol`) already carries
  `method: HttpMethod`, `url`, `headers`, `body` — the wire format is fine;
  only the host impls are incomplete. Guest HTTP requests are relayed by the
  firmware `ipc_handler` / desktop wasm runner using these same types, so
  fixing the firmware client automatically enables guest POST/PUT once done.
- `reqwless` supports methods via `reqwless::request::Method` and request
  bodies via `RequestBuilder::body(&mut [u8])` (body buffer must outlive the
  send call — keep that in mind in the streaming helper).

### Work items

1. **Firmware method/body support** in `perform_http_request_streaming`:
   map `app::protocol::HttpMethod` → `reqwless::request::Method` (Get/Post/
   Put/Delete). Send `http_request.body` as the request body when the method
   is POST/PUT (and non-empty); still send no body for GET/DELETE. Log the
   method in the existing `HTTP: request ...` debug line.
2. **Delete the duplicate `HttpEvent`** in `firmware/src/utils/http.rs`;
   use `app::protocol::HttpEvent` everywhere (it has the `Error` variant the
   platform channel contract expects). Fix `perform_http_request_channel` to
   send `Error` (not `Done`) when the underlying request fails. Check every
   caller (grep for `perform_http_request` /
   `perform_http_request_channel` / `perform_http_request_streaming` —
   native app HTTP in `firmware/src/native/common.rs` and the ipc handler are
   the likely callers) for assumptions about the old enum.
3. **Desktop parity**: forward `req.headers` into the ureq request (all
   methods). Keep the streaming `Meta/Chunk/Done/Error` sequence identical to
   firmware.
4. **Tests**: add app-level unit tests for `HttpRequest`/`HttpResponseMeta`
   serde round-trips (JSON wire compatibility, including a POST request with
   a body — this is what WASM guests send). Firmware impl itself is
   network-bound and can't be unit tested; document a manual on-device
   verification step in your report (or run it if hardware is available:
   upload a small app that POSTs to a local server).
5. **Update AGENTS.md**: remove "High Priority 1" from Remaining Work; note
   that the firmware client honours method/body/headers.

### Acceptance

- `just build_firmware` and desktop build succeed.
- Grep shows a single `HttpEvent` definition (in `app::protocol`).
- Debug logs show the method; a desktop POST (e.g. an app hitting a local
  test server) carries body and headers correctly — demonstrate with a quick
  manual run and paste the log lines in your report.
- `cargo test -p app` green.

### Report

Mapping table for methods, reqwless body API details you used, test names,
manual verification output.

---

## Block 4 — Firmware platform hardening: OTA/CPU parking, power loop, logging hygiene

### Goal

Close out AGENTS.md "High Priority 3 + 4" and the related crud: a single,
safe CPU-parking story for flash writes (currently `ota_begin` parks the app
core and **never unparks it on failure paths**, and `local_fs.rs` repeats
`CpuControl::new(unsafe { CPU_CTRL::steal() })` five times); a background
power-monitoring work loop with `WatchedValue`; and `println!`/emoji removal
from firmware code. Also resolve the 32 KB local_fs TODO.

### Background

- `firmware/src/platform/hardware.rs::ota_begin` creates a `CpuControl` via
  `steal()`, parks `AppCpu`, then drops the control — the core stays parked
  if any later OTA step fails (the device only recovers by reboot).
  `ota_write_chunk`/`ota_commit` write flash with **no** parking protection.
- `firmware/src/utils/cpu_guard.rs` has a working RAII `CpuGuard` but it is
  only used by `local_fs.rs`, which re-steals `CPU_CTRL` in five places.
  The guard logs via `esp_println::println!` and contains an emoji comment —
  against repo style.
- `firmware/src/utils/ota.rs`: `Ota` takes `&'a mut FlashStorage`, has a
  no-op `pub fn free(self)`, and hardcodes otadata offsets `0xd000`/`0xe000`
  without documentation. (Note: `ota_begin`/`ota_commit` are also the wrong
  shape for the "park for the whole OTA" semantics esp-idf uses — see work
  item 1.)
- `firmware/src/platform/power.rs`: lazy `ensure_init` + on-demand polling in
  `get_status()`; logs an `info!` **on every poll** including a full status
  dump. No background task. AGENTS.md's LED work-loop pattern is the model to
  copy. (Verify no leftover `power_monitoring_task` exists elsewhere — grep
  says it doesn't.)
- `firmware/src/tasks/wasm/mod.rs` uses `println!` and returns
  `Result<(), anyhow::Error>` from a loop that can't fail.
- `firmware/src/utils/local_fs.rs` line ~227: `// TODO: 32KB hard limit for
  now`.

### Work items

1. **CPU parking, one owner**: introduce a small `CpuParker` utility
   (replace `cpu_guard.rs`) that owns the `CpuControl` **once** (created in
   `firmware/src/bin/rustagon.rs` and handed to `HardwarePlatform` and/or the
   storage manager), exposing
   `async fn parked<F: Future>(&self, fut: F) -> F::Output` that parks,
   awaits, and always unparks (RAII guard). Refactor all five `local_fs.rs`
   sites and the OTA methods to use it. For OTA, choose a design and document
   it: the safest is park-on-begin/unpark-on-commit-or-error tracked in a
   shared `Arc<Mutex<bool>>` (since `Platform` methods take `&self`), or
   per-write `parked(...)` calls — justify your choice in a comment. **Every
   error path must unpark.**
2. **`Ota` cleanup**: delete `free()`; document the otadata layout
   (`0xd000`/`0xe000` are the two otadata sectors) in a comment; tighten the
   API if trivial (e.g. compute `target_slot` once at begin).
3. **Power work loop**: `HardwarePowerManager::new(i2c, spawner)` spawns a
   `power_monitoring_task` (Embassy task, ~2 s cadence — same pattern as the
   LED manager's internal loop) that reads the BQ25895 and updates a
   `WatchedValue<PowerStatus>` (from `app::utils`, Block 2). `get_status()`
   returns the watched value's current state; add `wait_for_change()`
   following the `WatchedValue` API. Log only transitions (`debug!` per poll
   or nothing; `info!` when charging state / VBUS presence changes).
4. **Logging hygiene**: replace every `esp_println::println!` /
   `println!` in `firmware/src` (cpu_guard, wasm task, anywhere else grep
   finds) with `log::{info,debug,warn,error}!`. Remove emoji from comments
   and strings. If pre-logger boot output in `bin/rustagon.rs` genuinely
   needs `esp_println`, keep it there only and say why in a comment.
5. **`anyhow` in `wasm_host_loop`**: the loop cannot fail — change the
   signature to remove `anyhow::Error` (plain `()` or a bespoke error). Check
   whether `anyhow` is still needed elsewhere in `firmware` (native app HTTP
   helpers use it — keep only if it earns its keep; report usage).
6. **32 KB limit**: investigate and either raise it (littlefs buffer sizing)
   or replace the TODO with a documented named constant and rationale.
7. **Update AGENTS.md**: remove High 3 + 4 from Remaining Work; document the
   `CpuParker` ownership pattern (this replaces the "CpuControl::steal()"
   note); note the power manager work loop.

### Acceptance

- `just build_firmware` succeeds; `-r --lib` check clean.
- Grep: no `println!` in `firmware/src` outside `bin/rustagon.rs` boot (and
  that documented); no `CPU_CTRL::steal()` outside the single owner.
- Code review pass of your own diff: every `park` has a guaranteed `unpark`
  on all paths.
- Hardware: flash and run if you have a badge — verify power app, OTA
  (success + simulated failure mid-write unpark). If no hardware, state it.

### Report

Parking design choice + rationale; power loop implementation notes; what
happened to `anyhow`; 32 KB decision; hardware test results or absence.

---

## Block 5 — TCP session lifecycle: kill the `Box::leak`s

### Goal

Remove every per-session/per-connection `Box::leak` from the TCP path:
currently the SSH app leaks a `TcpEventChannel` per session
(`app/src/apps/ssh.rs:163`) and the firmware client leaks the
`TcpClientState`, `NetTcpClient` and `CmdChannel` **per connection**
(`firmware/src/platform/tcp.rs`) — a genuine PSRAM leak on every
connect/disconnect cycle. Replace the `&'static TcpEventChannel` trait
parameter with an owned, scoped session type.

### Background

- `app/src/platform/tcp.rs` defines `TcpClient::connect(&self, host, port,
  channel: &'static TcpEventChannel)`; `TcpEventChannel` is a bounded
  embassy channel. The trait docs admit the `'static` requirement is for the
  background pump.
- Firmware impl: `connect` allocates state/client/cmd-channel in
  `ExternalMemory` and `Box::leak`s them; a pump task
  (`tcp_pump_task`, spawned via `Spawner::for_current_executor()`) owns the
  `!Send` connection; a `CmdSlot` (`Arc<Mutex<(u64, Option<&'static
  CmdChannel>)>>`) with a generation counter detects stale pumps. None of
  the leaked allocations is ever freed.
- Desktop impl (`desktop/src/platform/tcp.rs`) presumably uses a thread per
  connection with its own channel — inspect and adapt to the new trait.
- Only the SSH app consumes TCP today (grep to confirm).
- Embassy tasks can own `!Send` data as **task arguments** — the pump
  already takes `mut conn: Conn` by value, which is the key to fixing this:
  the pump task's future can own the boxed state/client and free them when
  the task ends.

### Work items

1. **Redesign the trait** in `app/src/platform/tcp.rs`:
   - New public type `TcpSession` (cloneable handle) exposing
     `next_event() -> TcpEvent`, `try_next_event()`, `send(Vec<u8>)`,
     `close()`.
   - Trait becomes something like
     `fn connect(&self, host: String, port: u16) ->
       Pin<Box<dyn Future<Output = Result<TcpSession, ()>> + 'static>>;`
     the **platform impl creates the channel internally** and stores the
     receiver in `TcpSession`; the app never sees or leaks a channel.
     `Connected`/`Error` can be conveyed as the `Result` (simplest) or as
     channel events — pick one and document.
   - `send`/`close` move onto `TcpSession` (drop the separate trait methods,
     or keep the trait minimal — your design, keep it ergonomic).
2. **Firmware impl**: restructure so the pump task **owns** the connection,
   boxed state, and client (all as task args/future locals — the existing
   `for_current_executor` spawn keeps `!Send` sound), and the
   `CmdSlot`/generation pattern only holds the `Sender` half of the command
   channel (Sender is cloneable + Send). When the pump exits, everything is
   freed. Delete all three `Box::leak` calls. Preserve the generation-based
   stale-pump detection (it is good) — adjust it to the new ownership.
3. **Desktop impl**: same trait; keep or simplify the thread-per-connection
   approach; ensure the channel and socket are freed when the session is
   dropped/closed.
4. **Update the SSH app** (`app/src/apps/ssh.rs`): hold
   `Option<TcpSession>`; no `Box::leak`; disconnect drops/closes the session.
   Check `app/src/ssh/` for any other TCP assumptions.
5. **Update AGENTS.md**: remove the `&'static` leak pattern from the docs;
   document the new session ownership model and why the pump-task-owns-all
   shape works on a single-core executor.

### Acceptance

- `cd firmware && cargo build -r --lib` clean; desktop builds; app tests
  green (the ssh e2e test exercises the new trait).
- Grep: no `Box::leak` in `app/src/apps/ssh.rs` or
  `firmware/src/platform/tcp.rs`.
- Desktop: SSH connect/disconnect/reconnect cycle works and (observable via
  logs or a quick instrumented check) no unbounded growth.
- Hardware (if available): repeated connect/disconnect on-device, watch
  `print_memory_info` output stay flat.

### Report

Trait diff summary; firmware ownership diagram (one short paragraph);
desktop approach; leak verification evidence.

---

## Block 6 — Menu/app model: apps own their loop (`MenuApp::run` + spawner)

### Goal

Flip the app framework from menu-driven callbacks to **apps that own a
loop**, so push-driven apps (SSH, future chat/sensors) can `select!` on
input, events, timers and background work, and the menu stops being a nest
of duplicated `Either3/4` handlers. This is the single highest-leverage
structural change; do it once, do it cleanly. (Supersedes the old plan's
"1a/2a"; "1b/1c/2b" already landed.)

### Background

- Current trait (`app/src/apps/common.rs`): `render`, `init`,
  `handle_input`, `handle_event`, `tick`, `on_stop`, `on_shown`. The menu
  (`app/src/menu/mod.rs`) has three near-duplicate handlers
  (`handle_root_menu`, `handle_menu_app`, `handle_hosted_app`) each with
  nested `select3/select4`, duplicated stack-event polling, and duplicated
  nav-key injection (`device_key_to_nav` called in two places).
- Consequences: while an app awaits inside `handle_input` (SSH connect), the
  menu processes nothing — no boot button, no keys; apps cannot own timers
  or background tasks; there is no way to cancel a long operation.
- Apps today: `app_store`, `config`, `editor`, `files`,
  `hexpansion_viewer`, `input_test`, `ota_updater`, `power_info`,
  `wifi_scanner`, `ssh` (in `app/src/apps/`, wired via `MenuAppType` in
  `mod.rs`). The root menu is its own ad-hoc handler.
- `Platform` is `Send + Sync`; firmware futures are frequently `!Send`
  (embassy-net), desktop futures are `Send`. Any spawner abstraction must
  survive both.
- The `MenuApp` trait currently returns `AppAction::{Continue, Stop,
  LaunchWasm, LaunchNative}`; the stack machinery (`app/src/menu/state.rs`,
  `StackSignal`) works and should be preserved.

### Work items

1. **New trait** in `app/src/apps/common.rs`:
   ```rust
   pub trait MenuApp {
     fn render(&self) -> LcdScreen;
     async fn run(&mut self, ctx: AppRunContext) -> AppAction;
     async fn on_stop(&mut self) {}   // keep for pop-time cleanup
   }
   ```
   `AppRunContext` (constructed by the menu per invocation) multiplexes:
   - `next_input() -> MenuAppInput` (hex button, system/boot button, Stop)
   - `next_event() -> AppEvent` (hexpansion + device events)
   - a `tick`/`sleep(ms)` timer facility
   - `spawn(fut)` for background work (see 2)
   - access to the existing `platform`/`display`/`host_ipc_sender` (rename
     the old `MenuAppContext` or fold it in — decide, keep churn contained).
   Provide one shared helper (e.g. `select_input_event(ctx)`) so simple apps
   stay small, plus a temporary migration shim `run_from_callbacks` that
   adapts an old-style app's `handle_input`/`handle_event`/`tick` into
   `run()` — use it only to port incrementally; **end with zero apps on the
   shim**.
2. **Spawner**: add `AppSpawner` to `app/src/platform/` (new module), a
   cloneable handle obtained from `Platform` (new trait method,
   e.g. `Platform::spawner()`) — firmware provides an Embassy-spawner
   wrapper, desktop a thread/executor wrapper. Decide the
   `Send`-ness: firmware's `!Send` futures can only be spawned on the
   current executor (the existing `for_current_executor` pattern) — expose
   `spawn(Send fut)` plus `spawn_local` (firmware impls it via
   `Spawner::for_current_executor`, desktop via its thread pool), and
   document exactly when apps must use `spawn_local`. (Hint: in the new
   model most "background" app work is just `select!` in `run()` — the
   spawner exists for genuine long-lived helpers.)
3. **Rewrite the menu loop** (`app/src/menu/mod.rs`): collapse the three
   handlers into one `run_entry` per stack-top kind. The **root menu becomes
   a `MenuApp`** (a `RootMenuApp` holding options + selection, with the
   navigation logic moved into its `run()`). `MenuApp` entries:
   `ctx`, `app.on_shown`-equivalent (re-run entry), loop `run()`, translate
   `AppAction` (LaunchWasm/Native → send IPC + push `HostedApp`; Stop →
   `on_stop` + pop). HostedApp keeps its own pump handler but reuse a single
   `next_input`-style helper. Delete the duplicated stack-event checks and
   `device_key_to_nav` duplication — nav injection happens in exactly one
   place in `AppRunContext`.
4. **Port all ten apps** to `run()`. SSH is the interesting one: move
   `connect()` + handshake + terminal pump into the `run` loop with
   `select!` so boot-button cancels mid-handshake, and inbound data renders
   without waiting for input. Preserve the existing status/screen behaviour
   (the connect form, `fail()`, etc.).
5. **Update AGENTS.md**: rewrite the "App State Machines" section for the
   new model; update "How the Menu Moves Through Crates" if it changed; note
   the `AppSpawner` addition to the Platform trait.

### Acceptance

- All ten apps work on desktop (`just run_desktop`): launch, interact,
  leave, relaunch — no regression in behaviour.
- `cargo test -p app` green (ssh e2e through the new model).
- `cd firmware && cargo build -r --lib` clean; `just build_firmware` if you
  have the toolchain.
- Menu code substantially smaller: no `select3/select4` nesting beyond a
  single `select` in `AppRunContext` internals.

### Split point (if context runs out)

Land trait + menu rewrite + all non-SSH apps first, then SSH in a follow-up
session. State clearly in the report which half you reached.

### Report

Trait/context API as landed; per-app port notes (esp. SSH); menu LOC before/
after; any behavioural deltas found and fixed.

---

## Block 7 — `MockPlatform` + headless app driver + golden screens

### Goal

Make every built-in app testable in CI without hardware or a display: a
scripted `MockPlatform`, an `AppDriver` that runs an app's `run()` loop
against a script, and golden-screen assertions. (Old plan 9c / AGENTS "Low
2" — now unblocked because Block 6 made apps self-contained loops.)

### Background

- `app` is `no_std`; tests run on the host (`cargo test -p app`) where `std`
  is available to the test harness even though the lib is `no_std`.
- Existing tests: `app/src/keys.rs`, `app/src/ssh/*` (including an e2e that
  spins a real puressh server). The SSH engine can be driven against a
  **canned transcript** instead of a real server once a fake TCP exists.
- `LcdScreen` (display_types) derives serde — golden snapshots can be JSON
  fixtures.
- embassy-time offers a mock/test driver (check the embassy-time crate
  features in the pinned version — if the mock driver isn't usable here,
  introduce a tiny `Clock` indirection in `app::utils` instead; prefer the
  embassy mock so no production code changes).

### Work items

1. **`MockPlatform`** in a new feature-gated module (e.g.
   `app/src/testing/mod.rs`, feature `testing` enabled only in dev-deps; or
   a separate `app_test` support crate — decide based on how
   firmware/desktop might reuse it). It implements `Platform` with:
   - scripted `InputManager`/`SystemManager`/`HexpansionManager` backed by
     `EventQueue`s (Block 2 made these reusable), injectable via public
     methods;
   - `FakeTcpClient` speaking a canned transcript (bytes in/out queues);
   - `FakeHttpClient` with scripted `Meta/Chunk/Done` responses;
   - in-memory `Storage`/`Config` (HashMap-backed);
   - recording `DisplayManager` capturing every `signal(LcdScreen)` with a
     timestamp.
2. **`AppDriver`**: takes an app factory + script of steps
   `{inject button/event, advance(ms), expect screen}`; polls the app's
   `run()` loop to completion per step; collects screens. On `expect`,
   compare against golden JSON (serde round-trip of `LcdScreen`).
   Regenerate with an env var (e.g. `UPDATE_GOLDEN=1 cargo test -p app`).
   Store fixtures under `app/tests/golden/<app>/<scenario>.json`.
3. **Golden tests for every built-in app**: minimum launch-screen + one
   interaction (button press changing selection, etc.). SSH: drive the
   engine with the fake TCP + canned handshake transcript (port the existing
   e2e test to run without a real server; keep the real-server test if it is
   cheap).
4. **Wire into CI**: these are plain `cargo test -p app` — confirm the
   Block-1 `just test` recipe picks them up (it already runs `-p app`).
5. **Update AGENTS.md**: remove "Low 2" from Remaining Work; document the
   testing module and the golden-update workflow.

### Acceptance

- `just test` green, including golden tests for all apps.
- `UPDATE_GOLDEN=1` regenerates fixtures cleanly (no spurious diffs on
  re-run).
- The menu itself: add at least one driver test for `RootMenuApp` navigation
  (up/down/fire).

### Report

Module layout chosen; how the clock is mocked; list of tests per app; any
apps whose behaviour you had to stabilise to make snapshots deterministic.

---

## Block 8 — Shared widget toolkit (`app/src/ui/`)

### Goal

Stop every app hand-rolling text editing, lists and terminals. Build a small
`no_std` widget set in `app/src/ui/`, then port the Editor and SSH onto it.
(Old plan 3.)

### Background

- `app/src/apps/editor.rs` hand-rolls cursor/editing/shift logic;
  `app/src/apps/ssh.rs` hand-rolls a connect form (4 fields + focus) and
  `fail()` status; `app/src/ssh/terminal.rs` is a terminal renderer that the
  SSH app needs but future apps (logs viewer, serial) could reuse.
- `MenuAppContext::notify()` already gives toasts
  (`LcdScreen::Notification`) — don't rebuild that.
- Rendering targets: `Vec<TextBufferLine>` / `LcdScreen` (display_types);
  `display_renderer` handles drawing. Input arrives as `HexButton` and
  `KeyboardEvent`/`KeyCode` via `AppEvent` (arrow/Enter unified as buttons —
  AGENTS.md input section).

### Work items

1. **Widgets** (each with unit tests):
   - `ui/text_input.rs` — value, cursor, selection, backspace/delete/home/
     end, insert mode, shift state, clamp/wrap. Factor the field logic out
     of both Editor and SSH.
   - `ui/list.rs` — scrolling list with `selected` index (extract from root
     menu navigation after Block 6; `RootMenuApp` should use it too).
   - `ui/form.rs` — labelled fields + focus navigation (up/down/fire).
   - `ui/terminal.rs` — lift `app/src/ssh/terminal.rs` to a generic 32×8
     character-grid terminal with scrollback and cursor; SSH keeps only its
     SSH-specific key mapping.
   - `ui/progress.rs` — small progress bar (OTA/downloads).
2. **Port apps**: `EditorApp` onto `TextInput`; `SshApp` connect form onto
   `Form`/`TextInput`, its terminal onto `ui::terminal`. `RootMenuApp` onto
   `List` if cheap. Do not change user-visible behaviour.
3. **Update AGENTS.md** crate layout (`app/src/ui/`) and any app descriptions
   that change.

### Acceptance

- Side-by-side desktop run before/after: Editor and SSH connect form behave
  identically (typing, backspace, shift, arrows, focus).
- `cargo test -p app` green including new widget tests.
- No `std` leakage into `app` (widgets are `no_std`, `alloc` only).

### Report

Widget APIs; which apps were ported; behaviour diffs found during porting
(and how you verified none).

---

## Block 9 — App navigation with results + per-app storage

### Goal

Let apps push sub-apps and receive results back (file pickers, confirm
dialogs), and give every app a namespaced key-value store for settings and
state. (Old plan 4 + 8.)

### Background

- `AppAction::{LaunchWasm, LaunchNative}` fire-and-forget: no payload, no
  result. SSH can't open a file picker for `id_ed255.key`; nothing can show
  a confirm dialog.
- The stack (`app/src/menu/state.rs`) preserves parent state; the menu
  (`app/src/menu/mod.rs`, post-Block-6 shape) is where results must be
  threaded.
- Persistence: `DeviceConfig` exists via `ConfigHandle`, but apps have no
  per-app storage. SSH loses host/user/port and host-key fingerprints on
  relaunch. `LocalFsTrait` (embedded_tools) backs
  `StorageHandle` on both platforms — a KV store can be built on top of it.
- Blocks 6 (run model) and 8 (Form/pickers) are prerequisites for a clean
  implementation.

### Work items

1. **Push with result**:
   - Extend `AppAction` with `Push(AppType /* or AppId */, Option<AppParams>)`
     (typed serde payload — keep it small) and add a per-push result channel
     created by the menu (`Channel<_, AppResult, 1>`, `AppResult` enum:
     e.g. `Path(String)`, `Confirm(bool)`, `Cancelled`).
   - The parent awaits the channel inside its `run()` loop (add a
     `pending_result()` source to `AppRunContext` if Block 6 didn't leave a
     natural seam). The child signals by returning a new
     `AppAction::Result(AppResult)` or `Stop` + result; the menu delivers it
     to the parent's channel and resumes the parent.
2. **Pickers**: `FilesApp` gains a picker mode (constructor flag) returning
   the selected path on Fire; a tiny `ConfirmationApp` (message, Yes/No)
   returning `Confirm(bool)`. Wire SSH's key field to "[Pick key]"
   (launches the picker, fills the field on result).
3. **Per-app KV store**: `app::kv::KvStore` over `StorageHandle` — one JSON
   file per namespace (`/apps/<name>/kv.json`, atomic write: write tmp file
   then rename if the platform supports it, else document the fallback),
   with typed get/set helpers (`kv.get::<T>("key")` serde). Add a
   `kv()` accessor to `StorageHandle` or `MenuAppContext` (decide, keep
   ergonomic). Persist SSH host/user/port + **host-key fingerprint
   (trust-on-first-use)** across relaunches; refuse/confirm on fingerprint
   change.
4. **Tests**: KV round-trip unit tests (in-memory storage); a driver test
   for picker→result flow using Block 7's AppDriver.
5. **Update AGENTS.md**: new `AppAction` variants, KV pattern, app list.

### Acceptance

- Desktop: SSH → pick key via FilesApp → connect; host/user/port remembered
  on relaunch; changed-host-key prompt works (fake a changed fingerprint).
- `cargo test -p app` green; firmware `--lib` builds.

### Report

Wire shapes (`AppParams`/`AppResult`); KV file format + atomicity decision;
which apps adopted KV.

---

## Block 10 — Desktop platform parity (hexpansion/wifi simulation, icons, key repeat)

### Goal

Make the desktop emulator a faithful platform: simulated hexpansion
insert/removal, a fake WiFi environment, icon rendering verified, and key
repeat. (AGENTS "Medium 4", "Low 1"; old plan 11a.)

### Background

- `DesktopHexpansionManager` (`desktop/src/platform/common.rs`) is a stub:
  `next_event` returns `pending()`, `current_state` is empty. Only device
  (keyboard) events flow. `HexpansionViewerApp` therefore shows nothing on
  desktop.
- `DesktopWifiManager` is a stub (always `Offline`, empty scan). After
  Block 2 it owns a `WatchedValue`/queue — this block fills them.
- Icons: menu icons render via `include_rgb565_icon!` (procmacros) — verify
  whether they show on desktop at all; AGENTS.md flagged "icons empty on
  non-xtensa". Find the root cause (feature gate, missing data files, or
  renderer path) and fix.
- Key repeat: nothing repeats today; holding a key/button gives one event.

### Work items

1. **Hexpansion simulation**: a JSON per slot
   (`<data_dir>/hexpansions/port<1..6>.json`: `vid`, `pid`, `unique_id`,
   `friendly_name`, optional `driver` tag). `DesktopHexpansionManager::new`
   takes the data dir; a background thread polls the files (~1 s mtime
   check) and pushes `HexpansionEvent::Inserted/Removed` into the
   `EventQueue` added in Block 2. `current_state()` reflects the files.
   Document the format in the repo (a sample file in `desktop/data/`).
   Verify `HexpansionViewerApp` live-updates when a file appears/disappears.
2. **WiFi simulation**: `<data_dir>/wifi.json` (AP list with signal
   strengths). `scan()` returns it; `set_desired_state(Online)` flips the
   `WatchedValue` to `Online` (and back), so `wait_for_status_change`
   actually resolves — make `WifiScannerApp` and any wifi-dependent app
   fully exercisable on desktop.
3. **Icons on desktop**: investigate, fix, and verify the menu/app icons
   render on macOS. If it is a data/format issue, fix at the source; if the
   icon assets are firmware-only, decide how to include them for the host
   build (same macro path preferred — no divergent rendering).
4. **Key repeat**: implement repeat in the **app layer** (Block 6's
   `AppRunContext` input stream) so both platforms share the policy: on
   `Button(..)` pressed, start a timer (~400 ms initial delay, ~100 ms
   repeat); cancel on the corresponding released event. Do not repeat
   `Stop`/system button. Expose it so WASM-hosted apps could later opt in
   (they get `HexButton` events forwarded — note the seam).
5. **Update AGENTS.md**: remove "Medium 4" and "Low 1" from Remaining Work;
   document the simulation file formats.

### Acceptance

- On desktop: hexpansion viewer shows simulated slots; adding/removing a
  port file live-updates the screen; wifi scanner lists fake APs; icons
  visible in menus; holding Down in a list repeats selection movement.
- Firmware untouched or trivially so (`-r --lib` still clean).

### Report

Simulation format; icon root cause + fix; repeat tuning constants and where
they live.

---

## Block 11 — WASM SDK parity & tooling (games check, emulator removal, size audit)

### Goal

Close the WASM side out: verify every shipped app, remove the deprecated
emulator crate, add a size/symbol audit wired into CI, and confirm guest
HTTP POST works end-to-end (it should after Block 3 — this block proves it).

### Background

- `sdk/src/bin/` ships ~21 apps; several were added recently from a games
  push ("Add games (need checking)": tetris, asteroids, flappy, snake,
  oggcamp…) and have not been verified post-upgrade.
- `libs/emulator/` is deprecated (AGENTS.md); `just emulate_wasm` still
  exists. The desktop runner (`just run_desktop_app <name>`) is canonical.
- Guest HTTP: `sdk/src/http.rs` builds an `HttpRequest` (method-capable via
  `wasm_protocol::HttpMethod`); the host relays it through the platform
  HTTP client (firmware `ipc_handler`, desktop wasm runner). After Block 3
  the firmware client honours method/body — but no app exercises POST.
- There is no size audit in CI; AGENTS.md documents a manual `wasm-tools`
  procedure ("Measuring").

### Work items

1. **Run every app** in `sdk/wasm/` (post `just build_sdk`) with
   `just run_desktop_app <name>` and confirm `WASM: Program complete` /
   no panic / no abort in the log. Fix any app that misbehaves (games are
   the expected problem area). Keep fixes minimal and in the SDK's style —
   remember the size discipline (no std, `fast_sin/fast_cos`, etc.).
2. **Remove the legacy emulator**: delete `libs/emulator/`, the `just
   emulate_wasm` recipe, and all AGENTS.md/README references. Update the
   workspace `Cargo.toml` members. Ensure nothing else depended on it
   (grep).
3. **Size audit tooling**: add a `just audit_wasm` recipe (a small script or
   a `tools/` binary) that, for each `.wsm`, reports size and greps symbols
   for forbidden entries (`dlmalloc`, `std::panicking`,
   `compiler_builtins::libm` — indicators `std` leaked in). Wire it into the
   CI `check` recipe from Block 1. Output a compact table; fail on
   forbidden symbols or a growth threshold (pick a sensible budget per app,
   document it).
4. **Guest POST proof**: extend `sdk/src/bin/fetch.rs` (or add a small
   `post.rs` app) to make a POST with a body against a configurable URL;
   run it on desktop against a local test server (e.g. `python3 -m
   http.server` won't accept POST bodies — use a tiny netcat/python
   responder) and confirm the host relays method/body/response. No wire
   protocol changes should be needed — if you find one is, stop and report
   rather than changing the JSON format unilaterally.
5. **Update AGENTS.md**: emulator removal, audit recipe, per-app size
   budget.

### Acceptance

- All `sdk/wasm/*.wsm` apps run on desktop without panic (list results per
  app in the report).
- `libs/emulator` gone; `just emulate_wasm` gone; builds still green.
- `just audit_wasm` produces the table and is wired into CI.
- Guest POST verified end-to-end on desktop.

### Report

Per-app verification matrix; emulator removal checklist; audit tool design;
POST verification transcript.

---

## Block 12 — Interrupt-driven keyboard (requires hardware)

### Goal

Replace the 20 ms polling in the TCA8418 keyboard driver with the HS_H
interrupt line for zero-latency key events. (AGENTS "Low 4".)

### Background

- `firmware/src/platform/drivers/tca8418.rs` polls every 20 ms
  (`Timer::after(20ms)` loop); the chip's key-event interrupts are enabled
  (`enable_key_event_interrupt(true)`) but the host never consumes the
  HS_H pin.
- The driver pushes into the shared `DeviceEventQueue` and translates
  arrows/Enter to `HexButton` via `KeyCode::to_hex_button` — preserve all
  of that.
- Drivers self-terminate after 3 consecutive I2C errors (hexpansion
  removed) — preserve this.
- This block **cannot be verified without a badge** (or at minimum the
  hexpansion + scope). If no hardware is available, do the code work,
  clearly mark it untested, and report; do not claim success.

### Work items

1. Wire the hexpansion HS_H pin as an async GPIO interrupt (esp-hal
   `Input`/`ExtInputFuture` or the `interrupt` module depending on the
   pinned esp-hal API) at driver startup, per port, reusing whatever pin
   setup the hexpansion port logic already defines.
2. Driver loop: `select!(interrupt, Timer::after(1s /* health tick */))` —
   on interrupt, drain the FIFO into the event queue; keep the poll as a
   fallback when interrupt setup fails (log a warning and run the old loop).
3. Debounce/validate: the chip already debounces; ensure the interrupt
   handler tolerates spurious edges (re-read FIFO, ignore empty reads).
4. Preserve driver self-termination (3 consecutive I2C errors).
5. Measure key latency on hardware (log timestamps press→event) and report;
   note any power implications of the always-on interrupt.
6. Update AGENTS.md ("Interrupt-driven keyboard" no longer pending) with
   the chosen approach and latency numbers.

### Acceptance

- Firmware builds; on hardware, key events arrive with no polling latency
  (measured), no missed/repeated keys, hexpansion removal still cleans up
  the task.
- Desktop unaffected (`-r --lib` clean).

### Report

esp-hal API used, wiring, measurements, fallback behaviour.

---

## Block 13 — Final polish & documentation sync

### Goal

Close out every loose end: AGENTS.md made fully accurate, nightly-feature
and `anyhow` hygiene, leftover TODO sweep, recipe audit, and a final
all-green pass. This is the "make it professional" signature block.

### Background

- `AGENTS.md` has drifted: it still lists work that landed (Blocks 1–12
  remove most), and its crate layout omits `app/src/http/`, `app/src/ssh/`,
  `app/src/native/common.rs`, `app/src/utils/` (directory), `tcp.rs`, `ui/`
  (Block 8), etc. Reconcile fully.
- `app/src/lib.rs` uses `#![feature(async_trait_bounds)]` and
  `#![allow(async_fn_in_trait)]` plus `#![cfg_attr(feature =
  "extern-alloc", feature(allocator_api))]`. After Block 6's trait rewrite,
  re-check which of these are still necessary; `async fn` in traits is
  stable on recent toolchains (the `allow` suppresses the lint) — drop what
  the toolchain no longer needs, and document what remains and why
  (`extern-alloc`/`allocator_api` is likely load-bearing for PSRAM allocs).
- `anyhow` is used in `firmware` (`tasks/wasm/mod.rs` was addressed in
  Block 4; `native/common.rs` and a macro in `utils/mod.rs` remain). Decide
  on one policy (keep it as a firmware-only convenience, or replace with a
  tiny local error type) and apply it consistently.
- Leftover TODOs: `firmware/src/utils/local_fs.rs` (Block 4), any stragglers
  found by `grep -rn "TODO\|FIXME" --include="*.rs"` excluding
  `libs/wasmi/**` (third-party submodule — out of scope).
- `justfile` default listing and recipes must match reality after Blocks
  1/11 (audit each recipe still works; remove dead ones).

### Work items

1. **AGENTS.md full reconciliation**: walk every section against the code;
   fix file paths, module lists, removed crates, new modules; move all
   resolved items out of "Remaining Work" (the section should end nearly
   empty — whatever remains should be deliberate, small, and re-filed into
   PLANS as future work if it matters); refresh the Build section with the
   Block-1 recipes and CI.
2. **Nightly-feature audit** as described; remove `#![allow(async_fn_in_trait)]`
   only if the trait actually no longer triggers the lint.
3. **`anyhow` policy** applied across `firmware`.
4. **TODO sweep** across `app/`, `desktop/`, `firmware/`, `sdk/`,
   `libs/` (excluding `wasmi`): fix, or convert to documented named
   constants/justified comments.
5. **Recipe audit**: run `just check`, `just test`, `just lint`,
   `just build_sdk`, `just audit_wasm`, `just build_desktop`,
   `just build_firmware` (toolchain permitting) and fix anything stale.
6. **Update this file**: mark all blocks complete in the status table; move
   any genuinely new backlog items to a short "Future work" section at the
   end (refinements only — no tech debt should remain).

### Acceptance

- Every `just` recipe green; working tree clean; one commit.
- `grep TODO` finds nothing outside `libs/wasmi` and justified cases.
- AGENTS.md describes the code as it is — an agent reading only AGENTS.md
  could navigate and build the project without surprises.

### Report

Summary of what was stale in AGENTS.md, toolchain findings, anyhow decision,
TODO resolutions, and the final status-table diff.

---

## Future work (after all blocks land)

Deliberately deferred — nice-to-haves, not debt. Revisit in a later planning
pass, in rough order of value: capability-registry refactor of `Platform`
(only if it grows again), WASM guest timers/`ByteStream` growth, richer web
app features, Bluetooth/power-expansion drivers.
