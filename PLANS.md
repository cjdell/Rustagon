# PLANS — App-Platform Upgrades

What to build so that complex interactive apps — SSH, editors, terminal UIs,
chat, remote-control panels — become easy to write on Rustagon.

Grounding: the SSH client (`app/src/apps/ssh.rs`, `app/src/ssh/`) was the first
app that needed real network I/O, a multi-screen state machine, background data
flow, and big buffers. It surfaced structural gaps in the app framework. Each
plan below is motivated by what SSH had to hand-roll (or fight) and lists the
concrete change. Rough effort is per-item (S=days, M=weeks, L=multi-week).

The guiding principle: **apps should be state machines that own a loop, not
callback handlers that are driven by the menu.** Everything else follows.

---

## 1. App model: give apps a loop, not callbacks

Today a `MenuApp` is a set of callbacks the menu calls: `init`, `handle_input`,
`handle_event`, `render` (`app/src/apps/common.rs`). Problems SSH exposed:

- `SshApp::connect()` is an `async fn` awaited *inside* `handle_input`. While
  it runs, the menu processes **nothing** — no boot-button, no hex button, no
  keyboard. A 15 s handshake makes the badge look frozen.
- There is no push channel: inbound TCP data is only drained opportunistically
  when `pump_session()` happens to run after a button/keyboard event. Real
  incoming shell output sits in the channel until the user presses something.
- Apps cannot own timers, retries, or background work.

### 1a. `MenuApp::run(ctx)` — the app owns its loop (L)

Replace the callback trio with one async method:

```rust
pub trait MenuApp {
  async fn run(&mut self, ctx: AppRunContext) -> AppAction;
}
```

`AppRunContext` multiplexes the sources the app can `select!` on:

- `MenuAppInput` (buttons, boot button, Stop)
- `AppEvent` (hexpansion, keyboard, future device events)
- `timer` ticks / `sleep`
- app-private channels the app registered at `init`

The menu's job shrinks to: call `run()`, translate the returned `AppAction`,
manage the stack. `handle_input`/`handle_event` become internal to the app
(most apps just call a shared `select_input_event` helper). This is the single
highest-leverage change: it makes continuous, push-driven apps (SSH pump, chat,
sensor dashboards) natural, and lets the menu regain control (boot-button
preemption, watchdog) because it can run the app loop inside a `select!`.

### 1b. Lightweight first step: add `tick`/`wake` (S)

If a full `run()` rewrite is too big, add a minimal escape hatch first:
`MenuApp::tick(&mut self)` called by the menu on a fixed cadence (e.g. 50 ms)
and on every external event, so SSH can drain its TCP channel without waiting
for input. Cheaper, but apps still can't own timers or long background work.

### 1c. `Stop` is a lifecycle event, not an input (S)

Right now `MenuAppInput::Stop` must be handled by *every* app or it leaks
sockets/pumps (`SshApp::disconnect()` is hand-wired). Make the menu call an
optional `on_hidden`/`on_stop` before popping, with a default that sends Stop
to the input path. Add `on_shown` for apps that want to refresh on return.

---

## 2. Background tasks & timers for apps

SSH wants a TCP pump that runs continuously, but the app layer has no access to
the executor and no clock beyond `sleep`.

### 2a. Per-app background task spawn (M)

Give `AppRunContext` a `spawn(fut)` that runs a task on the platform executor
(for the lifetime of the app). SSH would spawn its read pump as a task instead
of a leaked channel + opportunistic drain. Needs a `Spawner` available in the
app crate (`app` is `no_std`; a platform-provided `AppSpawner` handle keeps
`embassy-executor` out of `app`). Include a way to stop/join on pop.

### 2b. Clock & timeout helpers (S)

Add to `app/src/utils`:
- `now()` — monotonic ms
- `sleep(ms)`, `select_timeout(fut, ms)`, `with_timeout(fut, ms, on_timeout)`
- `retry(attempts, delay, op)` for connect/reconnect loops

SSH's hand-rolled `select(channel.receive(), sleep(15_000))` loops become
one-liners.

---

## 3. A real widget toolkit

Every app hand-renders `Vec<TextBufferLine>` and hand-rolls input handling.
SSH built its own connect form (4 fields + cursor), and its own terminal. This
is the biggest source of per-app boilerplate.

### 3a. Shared widgets (M)

`app/src/ui/` with:
- `TextInput` — value, cursor, selection, backspace/editing, shift handling
  (factor out the field logic from `SshApp` and `EditorApp`)
- `List` / `MenuList` — the RootMenu scrolling list, reusable by any app
- `Form` — labeled fields with focus navigation
- `StatusBar` / `Toast` — error/info line (SSH's `fail()` and status string)
- `ProgressBar` / `Spinner` — for downloads/OTA
- `Scrollback<Terminal>` — lift `ssh/terminal.rs` into a shared widget so the
  editor, logs viewer, and future serial/terminal apps reuse it

### 3b. Layout model (M)

`render()` returns `LcdScreen` (a concrete enum). Add a tiny layout API on top:
`layout::Column`, `layout::Row`, `layout::Frame`, plus a `Painter` that draws
to a framebuffer. Apps describe structure; the renderer packs it into
`LcdScreen` (or draws directly for non-text screens). Enables the SSH terminal
to render an actual 32×8 character grid with a cursor, instead of simulating it
with `TextBufferLine` strings.

### 3c. Focus management (S)

A `Focus` type (index into a widget tree) so arrow keys / keyboard / hex
buttons map uniformly. Removes the bespoke `active`/`field_count` logic in
SSH and the repeated `selected` handling in the menu.

---

## 4. App↔app navigation with results

Apps can only launch a sub-app (`AppAction::LaunchWasm/LaunchNative`) and get
no result back. SSH cannot open a file picker to choose `id_ed25519.key`.

### 4a. Push with payload + result channel (M)

`AppAction::Push(AppId, params)` where the pushed app, on Stop, sends a result
back through a channel the parent owns. The menu thread the channel through
the stack entry. Enables pickers (`FilesApp` returns a path), confirmation
dialogs, and chained flows (select key → connect).

### 4b. Generic picker apps (S, once 4a exists)

Make `FilesApp` (and a new `KeyPickerApp`) callable as a modal returning a
path. SSH's key field becomes "pick a file" instead of typed text.

---

## 5. Capability-based platform instead of a monolithic trait

Every new subsystem (TCP, entropy, next: BLE, GPS, sensors) means adding a
method to `Platform` (`app/src/platform/traits.rs`) **and** implementing it in
firmware, desktop, and any future host. SSH's TCP and entropy additions touched
~10 files. The `Option<TcpHandle>`/`Option<HttpClientHandle>` pattern already
admits "not available on this host" — formalize it.

### 5a. `Platform` as a capability registry (M)

```rust
pub trait Platform: Clone + Send + Sync {
  fn capability<T: Capability>(&self) -> Option<CapabilityHandle<T>>;
}
```

New subsystems register as `Capability` implementations (a `static` table +
`TypeId`), so adding SSH/TCP again is: write the capability in `app`, provide
impls in firmware/desktop, done — no trait churn. Keep the existing manager
handles as the primary API; `Capability` is the escape hatch for one-off
subsystems. (Alternative, lighter: split `Platform` into 3-4 sub-traits and
let the host impl each — still churn but smaller.)

### 5b. Namespace the ever-growing handle set (S)

Group handles on the context: `ctx.display`, `ctx.input`, `ctx.net`,
`ctx.storage` instead of `ctx.platform.xxx_manager()`. Cosmetic but makes
`MenuAppContext` self-documenting and shortens SSH-style code.

---

## 6. Async I/O ergonomics

### 6a. A shared `ByteStream`/half-duplex channel type (M)

TCP data, HTTP bodies, WASM host IPC, and serial all move `Vec<u8>` chunks
through `Channel`s today. Define one `ByteStream` type (bounded, backpressured,
cloneable, with `read`/`write`/`close` and timeouts) in `app`, and back TCP,
HTTP and WASM IPC with it. Replaces the per-subsystem `TcpEventChannel`,
`HttpEventChannel`, etc., and the weird `&'static` leak requirement.

### 6b. Kill the `&'static` channel leaks (M)

`TcpEventChannel` is `Box::leak`ed per session because the pump needs `'static`.
With an owned task model (2a) or a scoped channel owner (the pump owns the
channel and streams out of it), no leak is needed. SSH.md already calls the
leak "accepted for now" — make it not needed.

### 6c. Timeouts & reconnection baked into a `net` helper (M)

`app::net::connect(host, port)` → resolves DNS, opens TCP, returns a
`Connection` with read/write/close and built-in idle + connect timeouts. SSH's
`HardwareTcpClient`/`DesktopTcpClient` differences (pump task vs thread) hide
behind it. Add `retry_with_backoff` for reconnect.

---

## 7. Memory & stack discipline as a platform feature

The SSH stack overflow (2720-byte `SshSession` held across awaits) was found by
hand and fixed by boxing into PSRAM (`alloc_ext::external_box`). Make this
systematic.

### 7a. Generalise the external-alloc helpers (S)

`app/src/alloc_ext.rs` already exists. Extend with:
- `external_vec_with_capacity`, `external_string`, `external_box_default<T>`
- a `LargeBuffer<T>` wrapper that pages big temporaries to PSRAM automatically
- a `#[no_mangle]`/log helper that reports internal vs PSRAM heap usage per app
  at runtime, so the next overflow is caught by a test, not on hardware.

### 7b. Stack usage auditing in CI (M)

Add a tool (like `tools/stack-usage`) that compiles the app with the Xtensa
toolchain and reports `size_of` of every `MenuAppType` variant and the async
frame sizes of `run`/`connect`. Fails CI when an app's stack footprint grows
beyond a budget. Prevents regressions like the SSH enum bloat.

### 7c. App memory budget (S)

A `static` budget registry: each app declares `fn memory_footprint() -> usize`
(heap + stack). The menu checks free PSRAM/internal RAM before launching and
shows "Out of memory" instead of crashing. Cheap insurance for a platform that
runs 10 apps + WASM guests on 72 KB internal / ~8 MB PSRAM.

---

## 8. Data & persistence for apps

### 8a. Per-app key-value store (M)

`ctx.storage().kv()` — namespaced key-value on the filesystem (JSON on
desktop/littlefs on firmware, like `DesktopConfigManager`/`ConfigFile`).
SSH would persist host/user/port and its trust-on-first-use host-key
fingerprint (currently lost on reconnect). Config app, OTA, and wifi already
want this; unify `DeviceConfig`/`ConfigFile` behind the same API.

### 8b. Structured settings per app (S)

A `serde`-based `AppSettings` derive + load/save, so an app's settings are one
struct instead of bespoke field juggling.

---

## 9. Observability, errors, testing

### 9a. `AppError` + toast/status (S)

A common `AppError` (with `ToDisplay`), a `ctx.notify(msg)` that renders into a
shared status area, and `Result`-typed app ops. SSH's `fail()`, OTA's error
screen, and the connect-screen error all become the same thing.

### 9b. Level-per-app logging (S)

SSH used `info!` for everything because debugging over serial. Add a
`debug!`/`info!` convention per app + a runtime log filter (already
`ESP_LOG`) and trim SSH's chatty lines to `debug!` once verified. Cheap, but
reduces noise for the next person.

### 9c. `MockPlatform` + headless app driver (M)

AGENTS.md lists `MockPlatform` as a low-priority todo — it's now high priority.
Build:
- `MockPlatform` (scripted inputs, fake TCP/HTTP/storage, virtual clock)
- an `AppDriver` that runs an app against a script of `(input, event, wait)`
  steps and asserts the resulting `LcdScreen` snapshots
- golden-screen snapshot tests for every built-in app

SSH's e2e test had to spin a real OpenSSH server; with a mock TCP that speaks
a canned SSH transcript, the engine can be tested in-CI without a network.

---

## 10. WASM SDK parity

WASM guests (`sdk/`) are a second app framework with different IPC, no widgets,
and a subset of platform services. Every widget/feature we add to built-in apps
should be reachable from WASM or the platform divides permanently.

### 10a. Shared protocol growth (M)

Extend `libs/wasm_protocol` so guests get: timers, a `ByteStream` IPC
(TCP/HTTP), the same `run()`-style main loop, and the widget primitives
(text input, lists). The host already routes hexpansion/device events to
guests — route app-level push events too.

### 10b. Emulator parity (S)

Make `libs/emulator` and the desktop runner execute the *same* `MenuApp::run`
model so a WASM app behaves identically under emulation and on-device (SSH's
"works on desktop, fails on firmware" class of bugs shrinks).

---

## 11. Input model upgrades

### 11a. Key repeat (S)

Menu-level auto-repeat for held hex buttons / keys, so the SSH terminal cursor
and editor keys repeat. Today it's all press-per-event.

### 11b. Rich keyboard mapping (S)

Factor the `KeyCode → HexButton` + `KeyCode::to_char(shifted)` mapping
(`app/src/keys.rs`) into a shared input layer the menu, apps, and WASM guests
all use, so arrows/Enter/Shift unification never needs reimplementing.

---

## Suggested sequencing

1. **1b/1c/2b/9a/9b** (S, low-risk) — immediate wins: lifecycle, timers,
   errors, logging. Do these before touching SSH again.
2. **3c + 3a** (S+M) — shared widgets; port `EditorApp` then `SshApp` connect
   form onto `TextInput`/`Form`.
3. **1a/2a** (L) — `MenuApp::run` + background tasks. The big one; unblocks
   push-driven apps and the menu watchdog.
4. **5a/6a** (M) — capability registry + `ByteStream`; then port TCP/HTTP/WASM
   IPC onto it and delete the `&'static` channel leaks.
5. **4a/8a** (M) — navigation results + per-app storage; gives SSH the
   file-picker key flow and persisted known-hosts.
6. **7b/9c** (M) — testing & stack CI; make the next SSH-size app safe by
   construction.
7. **10** (M) — WASM parity, once the core model is stable.

Everything in 1–3 pays off directly for SSH; 4–7 compound for every app after
it.
