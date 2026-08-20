# Architecture & Development Notes for Agents

## Current Architecture

The system is structured around a **Platform abstraction** (`app::platform::Platform` trait)
that provides trait-based access to hardware subsystems. The `app` crate contains all
platform-agnostic code; `firmware` and `desktop` crates implement the `Platform` trait
for their respective targets.

### Crate Governance Rules

```
app/        — MUST be no_std, MUST NOT depend on embassy-net, esp-hal, ureq, or any
              hardware-specific crate. MAY depend on embassy-sync, embassy-futures,
              embassy-time (for sleep). All domain logic lives here.
              CAN depend on: embassy-sync, embassy-futures, embassy-time,
              embedded-graphics, serde, display_types, embedded_tools
              MUST NOT depend on: embassy-net, esp-hal, esp-alloc, ureq, reqwless

firmware/   — ESP32-S3 host. Implements Platform using esp-hal, embassy-net, reqwless.
              MAY depend on embassy-net, esp-hal, esp-alloc, esp-storage, reqwless.
              MUST NOT contain menu logic or app domain code — that goes in app/.
              Thin wrappers (Embassy tasks) call into app/ crate.

desktop/    — macOS/linux host. Implements Platform using minifb, ureq.
              MAY depend on ureq, minifb. MUST NOT contain menu logic.
              Platform crate is I/O centric — no WASM awareness in platform/.
              WASM runtime lives in tasks/ (mirrors firmware structure).

wasm_protocol/
            — Wire protocol types shared between the host and the WASM SDK.
              MUST be no_std and depend only on serde. Contains only type
              definitions (enums + payload structs) — no logic, no embassy-sync,
              no channels, no host functions, no LcdScreen. The app crate is the
              source of truth; SDK-facing subsets live here so the guest never
              sees host-internal lifecycle messages.
```

### Build (use `just`)

**You MUST run every build command inside the `nix develop` environment.** It
provisions the pinned Rust stable toolchain (with the `wasm32-unknown-unknown`
target and rust-src), the ESP32-S3 Xtensa toolchain, `just`, and all other build
tooling. The flake's `cargo` shim dispatches `cargo +stable` to a stock stable
that has the wasm32 target installed and everything else to the ESP fork.

Never run `cargo`, `just`, or `rustup` build commands from a shell that is not
inside `nix develop`. Building from a bare shell (host rustup, the wrong
toolchain, or missing targets) produces confusing failures — e.g.
`error[E0463]: can't find crate for core` when `cargo +stable` resolves to a
stable without the wasm32 target, or wrong build results when the host
`rustup` override takes precedence.

**If you see `critical-section` errors** (e.g.
`error: You must set at most one of these Cargo features: restore-state-none,
restore-state-bool, ...` or `RawRestoreStateInner redefined`), **you are definitely
building outside `nix develop`.** The workspace's `Cargo.lock` pins
`critical-section` with the correct feature configuration that only takes effect
inside the nix flake. Running `cargo` directly from a host shell bypasses the
flake's dependency resolution and hits a broken registry version. Re-enter
`nix develop` and retry.

To enter the environment:

```sh
nix develop                          # drop into a shell with the correct toolchain
nix develop --command bash -c "..."  # run a single command inside the environment
nix develop --command bash -c "just" # e.g. list all build tasks
```

All build/deploy tasks live in the root `justfile`. The firmware recipes handle the
Xtensa toolchain (`source ~/export-esp.sh`) and the `firmware/.env` file for you:

```sh
just build_firmware        # ESP32-S3 firmware (release, bin rustagon)
just run_firmware          # build + flash over USB (espflash)
just build_sdk             # build all WASM apps -> sdk/wasm/*.wsm + manifest.json
just build_wasm file       # build a single WASM app
just build_manifest        # regenerate sdk/wasm/manifest.json from the .wsm files
just emulate_wasm fetch    # DEPRECATED — legacy libs/emulator crate; use `just run_desktop_app <name>` instead
just run_desktop_app fetch # run the desktop app, auto-starting a WASM app from sdk/wasm
just run_wasm fetch        # build SDK + upload an app to the device over HTTP
just upload_wasm fetch     # build SDK + upload an app as a file
just deploy_firmware       # build + package merged.bin + deploy OTA firmware
just deploy_sdk            # build + deploy WASM apps
just deploy_web            # build + deploy web frontend
just deploy                # firmware + SDK + web
```

CI recipes (no hardware required; the GitHub Actions pipeline runs exactly
these, plus a full `just build_firmware` link):

```sh
just check   # builds every crate that builds without hardware (desktop, app
             # in all feature combos, firmware --lib, tools, web bundle + vite)
just test    # cargo test -p app (unit + SSH end-to-end tests)
just lint    # clippy -D warnings on app (union feature set), desktop, and
             # the firmware lib target, plus the web import-convention check
```

`just check` builds the compressed web bundle (`web/bundle/index.html.gz`) first
if it is missing — the `web-bundle` feature that both desktop and firmware
enable `include_bytes!`s it. `just lint` runs the firmware clippy from inside
`firmware/` so that crate's `.cargo/config.toml` (xtensa target + build-std)
applies.

The CI workflow (`.github/workflows/ci.yml`) installs Nix, runs
`just check && just test && just lint`, then links the full firmware binary
(release-lto). The submodule `libs/wasmi` is fetched over HTTPS via an
`insteadOf` rewrite (it is declared with an SSH URL).

Prerequisites:
- `just` installed (`brew install just`).
- Firmware recipes need the Xtensa toolchain (`source ~/export-esp.sh`) and a
  `firmware/.env` file — the recipes source both.
- `just build_sdk` is the **only reliable way to build the WASM apps**: it sets the
  correct `RUSTFLAGS` (stack size 32768, `--initial-memory=65536`) and copies the
  binaries to `sdk/wasm/*.wsm`. A bare `cargo build` inside `sdk/` omits those flags
  and produces binaries that exceed the guest memory limits.
- The web app uses `deno task build` for building (via Vite) and `deno task compress` for creating the bundled assets for firmware embedding.

Checks without hardware are `just check`, `just test`, and `just lint`
(see above). Individually, if you need a single crate:

```sh
cd desktop && cargo build                       # macOS/linux (minifb window)
cd app && cargo build --features wasm-runtime   # app library
cd firmware && cargo build -r --lib             # firmware crate, no linker
```

Note: linking the firmware *binary* needs the Xtensa linker
(`xtensa-esp32s3-elf-gcc`), available inside `nix develop` (the flake
provisions it). `just build_firmware` does the full release-lto link; the
firmware's `.cargo/config.toml` sets the xtensa target + build-std, so run
cargo from inside `firmware/` (the just recipes do).

### Headless app testing (MockPlatform + golden screens)

Every built-in app is tested in CI without hardware or a display, via the
`app` crate's `testing` feature (`app/src/testing/`), the golden-screen
integration tests in `app/tests/golden.rs`, the KV round-trip tests in
`app/tests/kv.rs`, and the push/result-flow tests in `app/tests/menu_results.rs`
(which drive the real `menu_task` on a background thread — the menu future is
`!Send` by design, so it is created *inside* that thread, like desktop does).
This is what `just test` runs:

```sh
just test                              # == cargo test -p app --features testing
UPDATE_GOLDEN=1 just test              # regenerate the golden fixtures
```

`--features testing` is **only** used by the test build. It turns on
`embassy-time/mock-driver` (a deterministically-advanceable clock) and compiles
`app::testing`. It is never enabled by firmware/desktop, so the mock clock can't
conflict with their real timers. (Do **not** enable it alongside the `tokio`/`std`
features — two embassy-time drivers won't link.)

The pieces:
- **`app::testing::MockPlatform`** — a scripted `Platform`: input/system/hexpansion
  backed by `EventQueue`s, in-memory storage + config, canned HTTP/TCP, and a
  recording `DisplayManager` (read back with `mock.last_screen()` / `mock.screens()`).
  Inject with `mock.push_button(..)` / `push_boot()` / `push_device_event(..)`;
  seed with `mock.seed_file(..)` / `set_config(..)` / `set_wifi_scan(..)` / `set_hexpansion_state(..)`;
  script HTTP with `mock.http().json(url, body)`.
- **`app::testing::AppDriver`** — pins an app's `run()` loop and drives it one poll
  at a time. `driver.settle(ms)` polls to quiescence, advancing the mock clock 1 ms
  per poll (up to `ms`) so timers like `ctx.notify(..)` can fire. Apps that don't
  render at the top of `run()` (root menu, App Store, OTA, input test) rely on the
  menu's entry render — `AppDriver::new` reproduces that by signalling `render()` first.

Golden fixtures live under `app/tests/golden/<app>/<scenario>.json` (a pretty
`serde_json` round-trip of `LcdScreen`). `expect_screen` compares against them, or
rewrites them when `UPDATE_GOLDEN=1` is set — regeneration is idempotent (no spurious
diffs on re-run). Each test holds a process-global lock and resets the mock clock,
so the tests are safe under cargo's parallel test threads.

Notes:
- The push/result plumbing is covered end-to-end in `app/tests/menu_results.rs`
  (SSH → Files picker → path back into the form; Cancelled on dismiss; the
  confirm dialog via the menu; SSH settings persisted to the KV file).
- The SSH *engine* handshake is still covered by the real-socket e2e unit test
  (`app/src/ssh/tests.rs`). The golden tests cover the SSH *app* UI (connect form
  field editing + focus navigation, key-not-found error path) and the Editor's
  full editing surface (typing, shift, backspace, left/right/home/end, Enter
  line-split — `editor_editing`, `ssh_form_editing`) headlessly — a full canned
  handshake transcript is not worth maintaining.
- The `just lint` recipe does **not** enable `testing`, so the mock/driver code is
  not part of CI clippy. If you change `app/src/testing`, check it manually with
  `cd app && cargo clippy --features testing --tests`.

### Release Profiles

The root `Cargo.toml` splits release builds so host crates build fast while the
firmware and SDK stay as small as possible:

- `[profile.release]` — fast builds for host crates (`desktop`, `app`,
  `emulator`, `tools`): no LTO, `opt-level = 3`, `incremental = true`.
  Incremental only applies to workspace members and path deps, so registry
  crates compile exactly as before.
- `[profile.release-lto]` — size-optimized profile (fat LTO, `opt-level = 'z'`).
  The firmware `just` recipes build with `--profile release-lto`, and `build_sdk`
  does the same for the WASM binaries (producing them under
  `target/wasm32-unknown-unknown/release-lto/`). This profile **must** be used for
  the SDK instead of relying on RUSTFLAGS `-C lto=true` with the default release
  profile, which would conflict with cargo's `-C embed-bitcode=no`.
  Do **not** set `codegen-units` in this profile: the esp toolchain's Xtensa
  backend fails to compile `xtensa-lx-rt`'s inline asm when `-C codegen-units`
  is passed explicitly.

Only the `just` recipes (which set the profile explicitly) produce the intended
artifacts; a bare `cargo build -r` in a crate uses the fast (no-LTO) profile.

### Keeping WASM apps small

The goal is "a handful of kilobytes" per app (`bare` ≈ 100 B, `barefill` ≈ 1 KB,
`barecube` ≈ 5 KB). The bulk of bloat comes from `std` being linked into every
guest. Rules that keep it out:

- **Minimal `sdk/Cargo.toml` deps.** Every entry links into *every* bin target
  (even if the app never uses it). An unused std-based crate (e.g. `wasi`,
  `pasts`, `anyhow`) drags `std` into all apps. Only list what the shared lib
  (`sdk/src/`) actually uses.
- **`serde` must not enable its default `std` feature** — always
  `default-features = false, features = ["derive", "alloc"]`. Same for any dep
  with a `std` default feature.
- **No std-only dependencies.** If a dep needs `std` (e.g. `fixed_deque` wraps
  `std::collections::VecDeque`), replace it with a small hand-rolled equivalent
  (see the `TaskQueue` ring buffer in `sdk/src/tasks.rs`).
- **The lib crate provides `#[global_allocator]` (lol_alloc) and
  `#[panic_handler]`** (non-formatting), so std's dlmalloc allocator and panic
  machinery are never the link-time fallback. Every bin links the `sdk` lib
  crate, so this is automatic.
- **`f32::cos()`/`f32::sin()` will NOT compile once `std` is gone.** Use the
  SDK's compact `fast_sin`/`fast_cos`/`fast_sqrt` (`sdk/src/trig.rs`; the trig
  approx is ~3.7 KB smaller than libm because it skips `rem_pio2f`). `libm` is
  no longer a dependency — don't re-add a `std`-pulling dep to make inherent
  float methods resolve.

- **`tick` is exported by each app binary.** Most delegate to
  `sdk::tasks::runtime_tick`; apps with a custom `tick` (`barefill`,
  `barecube`) return `1` immediately so the runtime's message parsing and task
  queue never get linked into them.

- **The SDK uses no nightly features** (no `#[thread_local]` — the task
  statics are `SyncUnsafe` wrappers, sound because wasm32 is single-threaded),
  so the flake pins a stock *stable* Rust; the `cargo +stable` shim is how the
  justfile builds the SDK.
- **Do NOT change the wire protocol to binary.** The host↔guest IPC is
  serde_json / serde-json-core JSON, and the JavaScript runtime in `web`
  implements the same format. Any protocol change must stay JSON-compatible.
- **Strip the name section** — `build_sdk` passes `-C strip=symbols` (worth
  2–4 KB/app). Don't remove it.

Measuring: inspect a built `.wsm` with `wasm-tools` (or the section/function
parser used during the size audit) and confirm there are no `dlmalloc`,
`std::panicking`, or `compiler_builtins::libm` symbols — those indicate `std`
leaked back in. After any change, rebuild with `just build_sdk` and verify every
app still runs in the desktop emulator (`just run_desktop_app <name>`), checking
for `WASM: Program complete` and no panic/abort in the log.

### Platform Trait

The `Platform` trait is the central abstraction in `app/src/platform/traits.rs`:

```rust
pub trait Platform: Clone + Send + Sync + fmt::Debug {
  fn display_manager(&self) -> DisplayHandle;
  fn led_manager(&self) -> LedHandle;
  fn power_manager(&self) -> PowerHandle;
  fn wifi_manager(&self) -> WiFiHandle;
  fn input_manager(&self) -> InputHandle;
  fn system_manager(&self) -> SystemHandle;
  fn http_client(&self) -> Option<HttpClientHandle>;
  fn tcp_client(&self) -> Option<TcpHandle>;
  fn storage_manager(&self) -> StorageHandle;
  fn config_manager(&self) -> ConfigHandle<DeviceConfig>;
  fn spawner(&self) -> SpawnerHandle;   // background-task spawner (see AppSpawner below)
  async fn format_storage(&self) -> Result<(), FsError>;
  async fn software_reset(&self);
  async fn ota_begin(&self) -> Result<u32, OtaError>;
  async fn ota_write_chunk(&self, offset: u32, data: &[u8]) -> Result<(), OtaError>;
  async fn ota_commit(&self) -> Result<(), OtaError>;
}
```

Each manager returns a cloneable `Handle` that wraps an `Arc<dyn ManagerTrait>`.

### AppSpawner (background tasks)

`app/src/platform/spawner.rs` defines the `AppSpawner` trait + `SpawnerHandle`
(`Arc<dyn AppSpawner>`), obtained via `Platform::spawner()`.

- `spawn(Box<dyn Future + Send + 'static>)` — portable path. Firmware runs it on
  the app-core Embassy executor (`SendSpawner`); desktop on a background std
  thread driven by `futures::executor::block_on`.
- `spawn_local(Box<dyn Future + 'static>) -> Pin<Box<dyn Future>>` — for `!Send`
  futures. The returned future must be awaited **from an async context on the
  executor the caller is on**; firmware resolves the executor via
  `Spawner::for_current_executor()` (sound: single-core cooperative runtime, same
  pattern as the TCP pump). Desktop futures are `Send` in practice, so
  `spawn_local` behaves like `spawn` (a scoped `unsafe impl Send` wrapper moves
  the box to the worker thread).

In the run-loop model (below) most "background" app work is just `select!` in
`run()` — the spawner exists for genuine long-lived helpers that outlive one
`run()` invocation.

### HTTP Client Abstraction

`app/src/platform/http.rs` defines the streaming HTTP abstraction:

- **`HttpClient` trait** — object-safe, `Send + Sync`. Method takes a request + shared channel.
- **`HttpEventChannel`** — `Channel<CriticalSectionRawMutex, HttpEvent, 2>` for streaming events.
- **`HttpEvent`** enum — `Meta(HttpResponseMeta)`, `Chunk(Vec<u8>)`, `Done`, `Error`.
- **`HttpClientHandle`** — `Arc<dyn HttpClient>` wrapper, cloneable, useable from Platform.

Usage pattern (channel-based streaming):

```rust
let channel = HttpEventChannel::new();
join(
  http_client.request(req, &channel),
  async {
    loop { match channel.receive().await {
      HttpEvent::Meta(meta) => { /* handle meta */ }
      HttpEvent::Chunk(chunk) => { /* write chunk */ }
      HttpEvent::Done => break,
      HttpEvent::Error => { /* handle error */ }
    }}
  },
).await;
```

**Why channel-based, not callbacks?** Callbacks with async closures can't be expressed in
object-safe traits. A shared `Channel` lets the producer (HTTP impl) and consumer (caller)
communicate concurrently without boxing complex closures.

**Firmware:** implemented via `reqwless` + `embassy-net::Stack`. The `HardwareHttpClient`
wraps `Stack<'static>` which is `!Send + !Sync`. Uses `unsafe impl Send + Sync` — same
pattern as `SendFilesystem` (sound because all access is serialized by the single-core
async runtime). `perform_http_request_streaming` (`firmware/src/utils/http.rs`) honours
`HttpRequest.method` (Get/Post/Put/Delete), forwards `HttpRequest.headers`, and sends
`HttpRequest.body` as the request payload for non-empty POST/PUT requests (no body for
GET/DELETE). It returns `Ok(())`/`Err(())`; the caller (the `HttpClient` impl) maps that
to `HttpEvent::Done`/`HttpEvent::Error`. The single source of truth for `HttpEvent`
(including its `Error` variant) is `app::protocol::HttpEvent`.

**Desktop:** implemented via `ureq` (blocking HTTP in a background thread). Forwards
`req.headers` for all methods and sends the body for POST/PUT — same `Meta/Chunk/Done/Error`
streaming sequence as the firmware client.

### WASM Wire Protocol (`libs/wasm_protocol`)

The types exchanged between the host runtime and a WASM guest live in a dedicated
shared crate, `libs/wasm_protocol` (`no_std`, serde + alloc only). It contains only
*wire-facing* types, derived from the app crate as the source of truth:

- Payloads: `HexButton`, `HttpMethod`, `HttpRequest` (app shape: `method`, `url`,
  `headers`, `body`), `HttpResponseMeta`.
- `WasmIpcMessage` (guest → host wire): only `HttpRequest(HttpRequest)`.
- `HostIpcMessage` (host → guest wire): `HexButton`, `HttpError`, `HttpResponseMeta`,
  `HttpResponseBody(Vec<u8>)`, `HttpResponseComplete`.

**The SDK never sees lifecycle messages.** `sdk/src/protocol.rs` re-exports
`wasm_protocol::*` next to its `extern "C"` host-function block. The start/stop and
display-sync variants that the runtime uses internally live only in the app crate,
wrapped around the wire enums so the wire format stays byte-identical:

```rust
// app/src/protocol.rs
pub enum HostRuntimeCommand { StartWasm(String), StartWasmWithBuffer(Vec<u8>), StartNative(String), Stop }

pub enum WasmIpcMessage {          // host-internal superset
  Wire(wasm_protocol::WasmIpcMessage),   // came from a guest
  Started, MenuAppStarted, Stopped, LcdScreen(LcdScreen),
}
pub enum HostIpcMessage {          // host-internal superset
  Runtime(HostRuntimeCommand),           // menu → wasm_host_loop (never a guest)
  Wire(wasm_protocol::HostIpcMessage),   // host → guest
}
```

`wasmi_runner` (`app/src/wasm/mod.rs`) is the boundary: it deserializes guest bytes
into `wasm_protocol::WasmIpcMessage` and wraps them in `Wire(..)`, and it serializes
only the *inner* wire enum when delivering `Wire(..)` host messages to a guest. The
`Wire`/`Runtime` wrappers are never serialized to the guest, so changing the superset
enums does not affect the wire format.

Adding a new guest-facing variant means updating `libs/wasm_protocol` (the SDK picks
it up automatically); adding a host-internal one means touching only `app::protocol`.

### TCP Client (`app/src/platform/tcp.rs`)

Session-oriented abstraction — there is **no** `&'static` channel parameter and no
`Box::leak` anywhere in the TCP path:

- **`TcpClient` trait** — one method:
  `connect(&self, host: String, port: u16) -> Pin<Box<dyn Future<Output = Result<TcpSession, ()>> + 'static>>`.
  `Ok(session)` means the connection is established; `Err(())` covers DNS/connect
  failure. The returned future is deliberately `!Send` (firmware embassy-net sockets
  are `!Send`); the trait itself stays `Send + Sync` for the `Arc<dyn TcpClient>`.
- **`TcpSession`** — cloneable handle (`Arc<dyn TcpSessionBackend>`) with
  `next_event()`, `try_next_event()`, `send(Vec<u8>)`, `close()`. Events are
  `TcpEvent::{Data, Closed, Error}` — there is no `Connected` variant, the
  connect `Result` conveys that. The public `TcpSessionBackend` trait (each
  platform implements it) returns `!Send` futures, but the handle is
  `Send + Sync`, so app state (e.g. `SshApp`) stays `Send`.
- **`TcpEventChannel`** is now `EventQueue<TcpEvent, 16>` (cloneable, owned) —
  created per connection by the platform, cloned into the session, freed with
  the pump. Apps never see a raw channel and never leak one.

**Firmware ownership model (`firmware/src/platform/tcp.rs`):** the pump Embassy
task is the *sole owner* of everything for the connection: the `TcpConnection`,
the pool state + `NetTcpClient` (an `OwnedNetTcp` PSRAM box — the client borrows
the state and the connection borrows the client, so `Pump::drop` frees the
allocation only after the connection has been dropped), and both event/command
queues. The `CmdSlot` (`Arc<Mutex<(u64 generation, Option<CmdChannel>)>>`) holds
the *owned* command queue; the generation counter detects stale pumps, which a
new `connect` asks to close before replacing the slot. The session talks to the
pump only through the command queue (`Send`/`Close`); an `Arc<AtomicBool>` alive
flag (cleared by the last session `Drop` or by pump exit) guarantees command/event
sends give up instead of blocking forever on a queue nobody drains or reads, and
a 200 ms liveness tick in the pump's `select` ensures a dropped session can never
strand the pump. The pump is spawned via `Spawner::for_current_executor()`
(`!Send` connection; sound because the single-core executor serializes all
access). When the pump exits, everything is freed — connect/disconnect cycles do
not grow PSRAM.

**Desktop (`desktop/src/platform/tcp.rs`):** one `std::net::TcpStream` per session
plus a reader thread pushing `TcpEvent`s (lossy 5 ms backoff, same as before). The
writer sits in an `Arc<Mutex<Option<TcpStream>>>` owned by the session; dropping
the last handle (or `close()`) shuts the socket down, which ends the reader thread
and frees everything. A unit test exercises connect/send/echo/close/reconnect.

### Completed Subsystems

#### 1. LED Management (`app/src/platform/led/`)

**Structure:**
- `traits.rs` - `LedManager` trait with `request(LedRequest)` method
- Hardware impls in `firmware/src/platform/led.rs`, mock in `desktop/`

**Design Pattern:**
- Manager spawns its own background work loop via Embassy task
- Internal channel for requests (completely encapsulated)
- No external channel management needed

#### 2. Power Management (`app/src/platform/power.rs`)

**Structure:**
- `PowerManager` trait with `power_off()`, `get_status()` and `wait_for_change()` async methods
- Hardware impl wraps the BQ25895 charger IC in `firmware/src/platform/power.rs`

**Design Pattern:**
- Async trait with `Pin<Box<dyn Future>>` for dyn compatibility
- `HardwarePowerManager::new(spawner, i2c)` spawns a `power_monitoring_task` work
  loop (~2 s cadence, same pattern as the LED manager) that owns the polling
  cadence and publishes into `WatchedValue<PowerStatus>` (from `app::utils`).
  `get_status()` returns the current state; `wait_for_change()` suspends until
  the next update. Callers never poll the chip on demand. The task logs only
  transitions (charging / VBUS presence / battery fault), not every poll.
- `power_off()` takes the `Arc<RwLock<CriticalSectionRawMutex, Bq25895>>`
  directly to disable the battery FET — prefer `RwLock` with
  `CriticalSectionRawMutex` for all internal synchronization

#### 3. WiFi Management (`app/src/platform/wifi/`)

**Structure:**
- `traits.rs` - `WiFiManager` trait with scan, connect, status methods
- `wifi_monitor_task` in `firmware/src/tasks/` consumes status updates

**Pattern:** `WatchedValue<WifiStatus>` for state notifications. Single source of truth,
supports `get()`, `set()`, `wait_for_change()`.

#### 4. HTTP Client (`app/src/platform/http.rs`)

- `HttpClient` trait with channel-based streaming
- `HttpClientHandle` wrapping `Arc<dyn HttpClient>`
- Firmware: `HardwareHttpClient` using `reqwless`
- Desktop: `DesktopHttpClient` using `ureq`

### Event Delivery Pattern (EventQueue)

`WatchedValue<T>` covers *state* (latest value wins). For *events* where every occurrence
matters (button presses, IRQ notifications), use `EventQueue<T, N>`.
Both live in `app/src/utils/sync.rs` (re-exported as `app::utils::{WatchedValue, EventQueue}`)
and are platform-agnostic (embassy-sync only), so every host uses the same primitives.

- Owns its `Channel` behind an `Arc` - no `static` channel and no `Sender`/`Receiver`
  plumbed through `main`; the manager creates the queue in `new()` and clones it into
  its spawned task (or, on desktop, hands a clone to the minifb thread).
- Consumer: `queue.next().await` - suspends until an event arrives, never polls.
- Producer: `push().await` (backpressure) or `try_push()` (lossy, safe from sync code).
- Cloneable, so it works naturally with `Arc<dyn Trait>` handles.

Used by `HardwareSystemManager` (boot button), `HardwareInputManager` (hex buttons),
`HardwareHexpansionManager` (device events from drivers), `HardwareWifiManager`
(`WatchedValue<WifiStatus>`), and the desktop managers (`DesktopInputManager`,
`DesktopSystemManager`, `DesktopHexpansionManager`, `DesktopWifiManager`).

**Never write a `loop { check_shared_vec(); Timer::after(..).await }` in a manager** -
that both adds latency and hogs locks. Use `EventQueue` (events) or `WatchedValue`
(state) instead.

#### 5. Hexpansion Detection (`app/src/platform/hexpansion.rs` + `firmware/src/platform/hexpansion.rs`)

**Detection:** An Embassy polling task scans the 6 hexpansion I2C ports (TCA9548A
channels 1–6) every 2 seconds for EEPROMs. On finding one, it reads the 32-byte
`"THEX"` header (magic + VID + PID + unique_id + friendly_name + checksum) and
emits `HexpansionEvent::Inserted`/`Removed`.

**Shared state:** Slot state is stored in `Arc<Mutex<RefCell<[Option<HexpansionInfo>; 6]>>>`
shared between the polling task and the manager.

**I2C bus topology:** One physical I2C bus split into 8 virtual buses by a TCA9548A
mux (0x77). Port 0 = top bus (frontboard), ports 1–6 = hexpansion, port 7 = system.

```rust
pub trait HexpansionManager: Send + Sync + fmt::Debug {
  fn next_event(&self) -> Pin<Box<dyn Future<Output = HexpansionEvent> + Send + '_>>;
  fn try_next_event(&self) -> Option<HexpansionEvent>;
  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)>;
  fn next_device_event(&self) -> Pin<Box<dyn Future<Output = DeviceEvent> + Send + '_>>;
  fn try_next_device_event(&self) -> Option<DeviceEvent>;
}
```

#### 6. Device Driver Framework (`firmware/src/platform/drivers/`)

When a hexpansion is detected, its VID:PID is looked up in a static driver table.
If a match is found, a driver task is spawned with a `DeviceIo` handle:

```rust
pub struct DeviceIo {
  pub port: u8,
  pub i2c: DeviceI2c,      // opaque I2C handle (wraps MaskedI2cBus)
  pub vid: u16,
  pub pid: u16,
}
```

`DeviceI2c` implements `embedded_hal::i2c::I2c` so existing driver crates (like
`tca8418`) can use it directly:

```
DeviceI2c → Arc<dyn DeviceI2cOps> → I2cBusWrapper → MaskedI2cBus → TCA9548A mux → ESP I2C
```

Drivers push events into a shared `EventQueue<DeviceEvent, 32>`:

```rust
pub type DriverFactory = fn(DeviceIo, DeviceEventQueue, Spawner);
pub struct DriverEntry { pub vid: u16, pub pid: u16, pub factory: DriverFactory }
```

Registration in `firmware/src/bin/rustagon.rs`:
```rust
const DRIVER_TABLE: &[DriverEntry] = &[
    DriverEntry { vid: 0xBAD3, pid: 0x4EEB, factory: tca8418_driver_factory },
];
```

**Task lifecycle:** Drivers self-terminate after 3 consecutive I2C errors
(hexpansion removed), preventing stale tasks on re-insertion to a different port.

## Current Crate Architecture

```
rustagon/
├── app/                              # Platform-agnostic application library (no_std)
│   ├── apps/                         # All menu apps (generic over P: Platform)
│   │   ├── common.rs                 #   MenuApp trait (render/run/on_stop), AppRunContext, AppInput/AppEvent/AppRunEvent,
│   │   │                              #   MenuAppContext<P>, AppAction (incl. Push/Result), AppParams/AppResult, ResultChannel
│   │   ├── app_store.rs              #   AppStoreApp<P>
│   │   ├── confirm.rs                #   ConfirmationApp<P> (Yes/No dialog; returns AppResult::Confirm)
│   │   ├── config.rs                 #   ConfigApp<P>
│   │   ├── editor.rs                 #   EditorApp<P> (text editor on ui::text_input, consumes DeviceEvent::Keyboard)
│   │   ├── files.rs                  #   FilesApp<P> (also a file picker in Push mode: AppParams::PickFile → AppResult::Path)
│   │   ├── hexpansion_viewer.rs      #   HexpansionViewerApp<P> (shows slot state)
│   │   ├── input_test.rs             #   InputTestApp<P>
│   │   ├── mod.rs                    #   MenuAppType<P> enum + list/load functions
│   │   ├── ota_updater.rs            #   OtaUpdaterApp<P>
│   │   ├── power_info.rs             #   PowerInfoApp<P>
│   │   ├── ssh.rs                    #   SshApp<P> (connect form on ui::form, key file picker, host-key TOFU, KV-persisted settings; shell on ui::terminal)
│   │   └── wifi_scanner.rs           #   WifiScannerApp<P>
│   ├── kv.rs                         # Per-app key-value store (one JSON file per namespace over StorageHandle)
│   ├── menu/                         # Full menu system (async fn, not Embassy task)
│   │   ├── mod.rs                    #   menu_task<P: Platform>(), RootMenuApp, run_* entry fns (incl. Push/Result delivery)
│   │   ├── state.rs                  #   AppStackEntry<P> (with pending_result slot), StackEntryType, StackEvent, StackSignal
│   │   ├── types.rs                  #   MenuRunnerContext<P>, MenuOption, AppLoader<P>
│   │   └── menus.rs                  #   get_root_menu_options, MenuProvider trait, StaticMenu
│   ├── native/                       # Native app types (stub, empty in app crate)
│   ├── platform/                     # Platform trait + 10 handle/manager pairs + HttpClient + AppSpawner
│   │   ├── traits.rs                 #   Platform trait (incl. hexpansion_manager, spawner)
│   │   ├── display.rs                #   DisplayManager trait + DisplayHandle
│   │   ├── hexpansion.rs             #   HexpansionManager trait + HexpansionHandle + DeviceIo + DeviceI2c
│   │   ├── http.rs                   #   HttpClient trait + HttpClientHandle + HttpEventChannel
│   │   ├── input.rs                  #   InputManager trait + InputHandle
│   │   ├── led.rs                    #   LedManager trait + LedHandle + LedError
│   │   ├── power.rs                  #   PowerManager trait + PowerHandle
│   │   ├── spawner.rs                #   AppSpawner trait + SpawnerHandle (background tasks)
│   │   ├── storage.rs                #   StorageHandle, ConfigHandle<State>
│   │   ├── system.rs                 #   SystemManager trait + SystemHandle
│   │   └── wifi.rs                   #   WiFiManager trait + WiFiHandle + WifiStatus
│   ├── protocol.rs                   # Host-internal IPC supersets (WasmIpcMessage/HostIpcMessage,
│   │                                  #   HostRuntimeCommand) wrapping wire enums from wasm_protocol
│   ├── types.rs                      # Domain types (HexButton re-exported, LedRequest, DeviceConfig,
│   ├── ssh/                          # no_std SSH engine (puressh sans-IO handshake/auth/channel)
│   │   ├── mod.rs                    #   SshSession, SshEvent, PlatformRng
│   │   ├── keys.rs                   #   key_to_bytes / hex_button_to_bytes (SSH terminal input mapping)
│   │   └── tests.rs                  #   Engine unit tests (incl. real-socket e2e handshake)
│   ├── types.rs                      # Domain types (HexButton re-exported, LedRequest, DeviceConfig,
│   │                                  #   HexpansionInfo, HexpansionEvent, DeviceEvent,
│   │                                  #   KeyboardEvent, KeyCode, etc.)
│   ├── ui/                           # Shared no_std widget toolkit (alloc only; no std)
│   │   ├── text_input.rs             #   TextInput: value + byte-index cursor (insert/backspace/delete/home/end/left/right/split)
│   │   ├── list.rs                   #   List<T>: items + selected index (root menu, pickers)
│   │   ├── form.rs                   #   Form/Field: labelled fields + action row, up/down/fire focus
│   │   ├── terminal.rs               #   Terminal: VT byte stream → TextBuffer lines (scrollback + cursor)
│   │   └── progress.rs               #   Progress → LcdScreen::BoundedProgress (OTA/downloads)
│   └── utils.rs                      # Sleep helper only
├── desktop/                          # Desktop platform (std, minifb, ureq)
│   └── src/
│       ├── main.rs                   # Minifb window, menu_task on bg thread
│       ├── platform/                 # DesktopPlatform, DesktopHttpClient (ureq)
│       │   ├── mod.rs                #   DesktopPlatform + DesktopHttpClient
│       │   ├── display.rs            #   DesktopDisplayManager (minifb-backed)
│       │   ├── fs.rs                 #   DesktopLocalFs (real directory-backed)
│       │   ├── input.rs              #   DesktopInputManager (keyboard) + DesktopSystemManager
│       │   ├── config.rs            #   DesktopConfigManager (JSON file-backed)
│       │   └── common.rs             #   DesktopLed/Power/Wifi/System managers (no-op impls)
│       └── tasks/                    # Desktop task wrappers (mirrors firmware/)
│           └── wasm/                 # WASM runtime (analogous to firmware's second core)
│               ├── mod.rs            #   spawn_wasm_runner, wasm_host_loop, run_program
│               └── context.rs        #   DesktopWasmHost + LCD_BUFFER
├── drivers/                          # Hardware driver IC crates
│   ├── bq25895/                      # BQ25895 charger IC driver
│   └── cy8cmbr3116/                  # CY8CMBR3116 capacitive touch controller driver
├── firmware/                         # ESP32-S3 host (thin platform impl + tasks)
│   ├── src/
│   │   ├── bin/rustagon.rs           # Entry point, hardware init, task spawning
│   │   ├── platform/                 # HardwarePlatform + manager impls
│   │   │   ├── hardware.rs           #   HardwarePlatform struct + Platform impl
│   │   │   ├── hexpansion.rs         #   HardwareHexpansionManager + I2cBusWrapper
│   │   │   ├── http.rs               #   HardwareHttpClient (wraps reqwless)
│   │   │   ├── drivers/              #   Device driver implementations
│   │   │   │   ├── mod.rs            #   DriverEntry, DriverFactory, DeviceEventQueue
│   │   │   │   └── tca8418.rs        #   TCA8418 keyboard driver (uses tca8418 crate)
│   │   │   └── ...                   #   display, input, led, power, storage, system, wifi
│   │   ├── tasks/                    # ESP32-specific Embassy tasks
│   │   │   ├── ipc_handler.rs        #   WASM IPC + HTTP event handling (separate from menu)
│   │   │   ├── menu/mod.rs           #   Thin Embassy wrapper → app::menu::menu_task()
│   │   │   ├── http/                 #   picoserve HTTP server
│   │   │   ├── wasm/                 #   WASM runtime on second core
│   │   │   └── ...                   #   net, wifi_monitor
│   │   └── utils/                    # ESP32-specific utils
│   │       ├── http.rs               #   perform_http_request_streaming (reqwless)
│   │       └── ...
│   └── Cargo.toml
├── libs/                             # Support crates (no standalone binaries)
│   ├── display_renderer/             # LcdState, FrameBuffer trait, icon drawing
│   ├── display_types/                # LcdScreen, Icon20/40, MenuLine, Image types
│   ├── embedded_tools/               # LocalFsTrait, ConfigFileTrait (no_std)
│   ├── emulator/                     # Legacy WASM emulator — deprecated; use the desktop app (`just run_desktop_app <name>`)
│   ├── esp32s3_embedded_tools/       # ESP32-S3 flash driver
│   ├── procmacros/                   # include_rgb565_icon!, partition_offset! macros
│   └── wasm_protocol/                # Wire protocol types shared with the WASM SDK (no_std)
├── sdk/                              # WASM SDK for badge apps (lib crate + bins)
│   ├── src/
│   │   ├── lib.rs                    #   crate root: modules + mk_static!
│   │   ├── gfx/                      #   zero-dependency drawing: Canvas, Rgb565, 5x7 font
│   │   ├── fmt.rs                    #   integer formatting without alloc::format!
│   │   ├── tasks.rs                  #   async runtime: spawn, yield_now, runtime_tick, HOST_IPC_CHANNEL
│   │   ├── trig.rs                   #   fast_sin / fast_cos / fast_sqrt
│   │   ├── http.rs                   #   make_http_request
│   │   ├── helper.rs                 #   host calls + println!/log_error! macros
│   │   ├── protocol.rs               #   extern "C" host fns + re-export of wasm_protocol::*
│   │   ├── sleep.rs / allocator.rs / panic.rs
│   │   └── bin/                      #   the apps (one .wsm each)
│   └── wasm/                         #   built .wsm files + manifest.json
├── web/                              # Web app for device management
│   ├── src/                          # Source code (Solid.js, TypeScript)
│   ├── public/                       # Static assets
│   ├── dist/                         # Build output (Vite)
│   ├── bundle/                       # Compressed bundle (inline assets, gzipped index.html)
│   ├── deno.json                     # Deno configuration (Vite, Solid.js)
│   └── vite.config.ts                # Vite configuration
├── tools/                            # Host build/deploy tools
│   ├── manifest-tool/                #   WASM manifest generator
│   └── uploader/                     #   Firmware upload tool
└── Cargo.toml                        # Workspace root

### What Must NOT Be In Each Crate

app/:
  ✗ embassy-net (Stack is !Send + !Sync — must stay in firmware)
  ✗ esp-hal, esp-alloc, esp-storage
  ✗ ureq, reqwless, or any HTTP client library
  ✗ Hardware-specific types (I2C, GPIO, SPI, etc.)
  ✓ MAY depend on embedded-hal for trait implementations (DeviceI2c implements
    embedded_hal::i2c::I2c) — but only the trait, never a concrete HAL driver.

firmware/:
  ✗ Menu logic (navigation, app listing, button handling)
  ✗ App domain code (AppStore logic, download, OTA)
  ✓ Thin Embassy task wrappers that call into app::

desktop/:
  ✗ Menu logic
  ✗ App domain code
  ✓ Platform impl (DesktopPlatform, DesktopHttpClient)
```

### Web App Import Conventions (`@lib`)

`web/src/lib` is a single importable unit exposed via the `@lib` alias (see
`web/deno.json`). The `@lib` alias must only be used by code **outside** `@lib`
(the app itself: `web/src/App.tsx`, `components/`, `routes/`). Code **inside**
`@lib` must never import from `@lib` — it creates a circular dependency on the
barrel `web/src/lib/index.ts` and blurs the boundary between the library and its
consumers.

- Inside `@lib`, use relative imports to other `@lib` modules, e.g.
  `import { sleep } from "../core/index.ts"` or `import { DeviceFileSchema } from "./common.ts"`.
- The public surface is still re-exported from `web/src/lib/index.ts` for
  consumers outside `@lib`.
- If you find a `from "@lib"` inside `web/src/lib/`, rewrite it as a relative
  import to the module that actually defines the symbol (check
  `web/src/lib/{core,device,wasm}/index.ts` to find it).

**Enforcement:** `cd web && deno task check-imports` (or `just check_web`) runs
`web/tools/check_imports.ts`, which parses every file in `src/lib` with the
TypeScript compiler API and fails (exit 1) on:

- `no-lib-self-import` — any file in `src/lib` importing the `@lib` barrel.
- `core-parent-import` — any file in `src/lib/core` importing outside
  `src/lib/core` (relative escapes such as `../device/...` or local aliases like
  `@components`). Bare specifiers that resolve to published npm modules
  (via `deno.json` `imports`, e.g. `valibot`, `@solidjs/router`) are allowed.

It runs automatically as part of `deno task build` (and therefore `just check`
and CI), and is available standalone as `deno task lint` (an alias of
`check-imports`) or `just check_web`.

### Build

See the **Build (use `just`)** section near the top of this document for the
recommended `just` recipes (firmware/SDK/deploy). Manual per-crate checks:

```sh
cd desktop && cargo build          # macOS/linux (minifb window)
cd app && cargo build --features wasm-runtime
cd firmware && cargo build -r --lib
```

See **Release Profiles** above — the firmware binary is built with
`--profile release-lto`, not plain `-r`.

### Formatting

The IDE formats on save with the repo's `rustfmt.toml` (`max_width = 140`,
`tab_spaces = 2`) using each crate's edition. `opencode.json` configures a
per-file formatter so opencode output matches; as a backup, after editing any
`.rs` file run:

```sh
rustfmt --edition 2021 --config skip_children=true <file>
```

`skip_children=true` ensures only the edited file is formatted (crate roots
won't drag in the rest of the crate). Do **not** use `cargo fmt` on the whole
workspace — it reformats hundreds of unrelated files.

**TypeScript/JavaScript (web/):** The IDE (Zed + Deno LSP) formats on save
using `deno fmt` conventions (from `web/deno.json`: `lineWidth = 140`,
`trailingCommas = onlyMultiLine`). `opencode.json` registers a `denofmt`
formatter (`.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`) so opencode output
matches the IDE and saving doesn't produce a huge reflow diff. After editing
any `.ts`/`.tsx` file in `web/`, run as a backup:

```sh
cd web && deno fmt <file>
```

`deno fmt` resolves `web/deno.json` from the file's location, so the project
settings apply regardless of the current working directory.

## How the Menu Moves Through Crates

1. `app::menu::menu_task<P: Platform>()` — the full menu system. Uses an **AppStack** to
   support multitasking: apps can launch sub-apps (WASM, native, or other menu apps), and
   the parent stays on the stack with its state preserved. Each stack-top kind has one
   `run_*` function; **apps own their event loops** (`MenuApp::run`, see below). Generic
   over any platform.
2. `firmware::tasks::menu::menu_task` — thin Embassy wrapper (~20 lines). Constructs
   `app::menu::types::MenuRunnerContext<HardwarePlatform>` and calls `app::menu::menu_task()`.
3. `desktop/src/main.rs` — calls `app::menu::menu_task()` directly via `thread::spawn`.

### AppStack Architecture

The menu maintains a `Vec<AppStackEntry<P>>` where each entry is one of:
- **RootMenu { menu: RootMenuApp }** — the root menu, an ordinary `MenuApp` that
  owns its options + selection (always at the bottom)
- **MenuApp(app)** — a built-in app (Files, Config, etc.) with its mutable state
- **HostedApp** — a WASM or native app running externally (second core on
  firmware, a thread on desktop)

The IPC handler (`firmware::tasks::ipc_handler`) communicates stack changes to the menu
via an `Arc<StackSignal>` (`Pushed(HostedApp)` / `Popped`), rather than through shared
state or global flags. This enables arbitrary nesting: a menu app can launch a WASM app
(which stays on the stack behind it), and that WASM app could trigger another native app,
and so on — each layer preserves its state on the stack.

**`StackSignal` vs channel:** The original implementation used an embassy-sync `Channel`
with multiple `Receiver` handles. On the firmware's cooperative single-threaded executor,
separate `Receiver` instances (created by each handler via `channel.receiver()`) could
both consume the same event — `handle_hosted_app`'s `select3` would consume a `Popped`,
then `handle_menu_app`'s `select3` on the next iteration would independently consume the
same event from the same channel, causing a double-pop. The fix replaces the channel with
`Arc<StackSignal>`, which wraps an `AtomicU8` (for the event value, consumed via
`swap(NONE)`) paired with an embassy-sync `Signal<CriticalSectionRawMutex, ()>` (for
async wake-up). The atomic `swap` is a single-consumer operation — only one reader sees
each event.

### App Run Loops (apps own their loop)

Apps own their event loop. The trait (`app/src/apps/common.rs`):

```rust
pub trait MenuApp<P: Platform> {
  fn render(&self) -> LcdScreen;
  /// The app's main loop. Returns when the user quits or wants to launch
  /// something. Re-entered by the menu on (re-)show and after `Continue`.
  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction;
  /// Called just before the app is popped (boot button, Stop, sub-app pop).
  async fn on_stop(&mut self) {}
}
```

`AppRunContext` (constructed by the menu per `run()` entry, borrows the
platform) multiplexes:
- `next_input() -> AppInput` — `Button(HexButton)` (hex button; keyboard
  arrows/Enter arrive as HexButtons via the platform) or `System(SystemMessage)`
  (boot button).
- `next_event() -> AppEvent` — `Hexpansion(..)` / `Device(..)`.
- `next() -> AppRunEvent` — the two combined into one enum (the common case).

`AppRunEvent::exit_action()` returns `Some(AppAction::Stop)` for the boot button
so every app gets "BOOP always leaves the app" for free; apps dispatch
everything else themselves.

`AppRunEvent` also carries `Result(AppResult)`: after an app returns
`AppAction::Push`, the menu re-enters its `run()` with the child's result
already buffered in the per-push channel, so the first `ctx.next()` on
re-entry delivers it (see below).

The menu (`app/src/menu/mod.rs`) has one `run_*` function per stack-top kind.
`run_menu_app` renders on (re-)entry (subsumes the old `init`/`on_shown` — apps
do refresh-on-show work at the top of `run()`), then loops
`select(app.run(ctx), stack_signal.receive())`: `Pushed` hides the app (hosted
sub-app, e.g. HTTP upload — it resumes when the hosted app pops), `Popped` is
defensive re-entry. `AppAction` translation: `Stop` → `on_stop` + pop;
`LaunchWasm`/`LaunchNative` → send IPC + push `HostedApp`;
`LoadMenuApp(name)` → `MenuAppType::load_app_async` + push (root menu uses this);
`Push(name, params)` → load with params + push + create a result channel;
`Result(r)` → `on_stop` + pop + deliver `r` to the parent;
`Continue` → re-render + re-enter `run()`.

### Sub-apps with results (`AppAction::Push` / `AppResult`)

Apps can push built-in sub-apps (file pickers, confirm dialogs) and receive
results back:

- **`AppAction::Push(name, AppParams)`** — `AppParams` is a small typed serde
  payload (`None`, `PickFile { message }`, `Confirm { title, message }`).
  `MenuAppType::load_app_async(name, ctx, params)` routes the params to the
  app; apps that don't take params ignore them.
- **The menu owns the delivery.** On `Push`, the menu creates a
  `ResultChannel` (`Arc<embassy Channel<CriticalSectionRawMutex, AppResult, 1>>`;
  `Arc` because embassy-sync 0.8 `Channel` is not `Clone`), attaches it to the
  *launcher's* stack entry (`AppStackEntry::MenuApp.pending_result`), and pushes
  the child. The child returns `AppAction::Result(r)` (or `Stop`, which is
  delivered as `AppResult::Cancelled`) and is popped; the menu `try_send`s the
  value into the parent's channel (capacity 1, so it never blocks) and
  re-enters the parent's `run()` with an `AppRunContext` wired to the channel —
  the first `ctx.next()` returns `AppRunEvent::Result(r)`. Apps that
  `select!` over `next_input()`/`next_event()` directly should drain with
  `ctx.try_next_result()` on (re-)entry.
- **Current consumers:** `FilesApp` picker mode (Fire on a file →
  `AppResult::Path`; back → `Cancelled`); `ConfirmationApp` (Yes/No →
  `AppResult::Confirm(bool)`; boot/back → `Cancelled`). SSH pushes the picker
  for its key field and the confirm dialog on a host-key change.
- At the root menu `Push` degrades to a plain launch (results are dropped —
  the root has no pending slot), so push from regular menu apps.

### Per-app KV store (`app/src/kv.rs`)

Every app gets a namespaced key-value store over `StorageHandle` via
`MenuAppContext::kv("namespace") -> KvNamespace`: one JSON file per namespace
at `/apps/<name>/kv.json`, with typed serde helpers
(`kv.get::<T>("key")` / `kv.set(key, value)` / `remove` / `contains`).

- **Atomicity:** `LocalFsTrait` has no `rename`, so writes are a single
  truncate-and-write serialised by the filesystem mutex (the documented
  fallback in `kv.rs` module docs). A torn file is tolerated: `load` treats a
  corrupt/unreadable file as *empty* and logs a warning, so a torn KV file
  costs an app its cached settings but never blocks it.
- **Current adopters:** `SshApp` (namespace `"ssh"`): the connect form
  (`host`/`user`/`key`/`port` under key `"connect"`, saved on each connect
  attempt, restored on first `run()` entry) and per-host host-key
  fingerprints (`host_key:<host>:<port>` → SHA-256 hex of the server host key
  in SSH wire format, captured by `SshSession::server_host_key()` from the
  first KEX reply).

**SSH host-key TOFU:** on `SshEvent::Connected` the app checks the stored
fingerprint: unknown → store + trust; match → continue; **changed → pause the
handshake** (the session is parked in `SshApp.session`/`tcp_session`), push the
`ConfirmationApp`, and on `Confirm(true)` resume the parked handshake via
`resume_handshake` (the new key becomes the stored one) or on
`Confirm(false)`/`Cancelled` close the connection and fail.

The **root menu is a `MenuApp`** (`RootMenuApp` in `app/src/menu/mod.rs`):
`AppAction::LoadMenuApp` launches built-ins; the boot button at root is a no-op
(selection state is preserved on the stack). There are no
`handle_input`/`handle_event`/`tick` callbacks and no `select3`/`select4` in the
menu — the only multiplexing is the single `select` inside `AppRunContext`.

**Nav-key injection happens in exactly one place** —
`apps::common::nav_button_from_device_event` (Escape→HexF, Tab→HexE, press +
release), used by the hosted-app pump and the root menu (both can only consume
HexButtons). Built-in menu apps receive raw keyboard events instead, so a Tab in
the Editor still types a tab.

**Push-driven apps** (SSH) `select!` over `ctx.next()` and their own work (a
`TcpSession::next_event()`, a download, a timer) in one loop: the boot button
cancels a mid-flight SSH handshake (each await point selects on `ctx.next()`),
inbound shell data renders without waiting for input, and long App-Store/OTA
downloads are interruptible the same way.

**`on_stop` runs on every pop** (boot button included) — SSH overrides it to
close its socket, not just on `Stop`.

## Desktop WASM Runner

The desktop WASM runner (`desktop/src/tasks/wasm/`) mirrors the firmware's WASM runtime
(`firmware/src/tasks/wasm/`) in both structure and responsibility:

| Concept | Firmware | Desktop |
|---------|----------|---------|
| Entry point | `second_core_task` (Embassy task on core 1) | `spawn_wasm_runner` (`std::thread::spawn`) |
| Message dispatch | `wasm_host_loop` (processes StartWasm/StartNative) | `wasm_host_loop` (processes StartWasm only) |
| Program execution | `run_program` (calls `app::wasm::wasmi_runner`) | `run_program` (calls `app::wasm::wasmi_runner`) |
| Platform host impl | `HardwareWasmHost` (SPI display, GPIO) | `DesktopWasmHost` (shared framebuffer, stdout) |
| IPC concurrent handler | Separate Embassy task (`ipc_handler`) | Inline in `run_program` via `join` |

The `Platform` trait and the `desktop/src/platform/` directory are I/O-centric (display,
input, storage, HTTP, etc.) and contain **no WASM awareness**. WASM concerns live entirely
in `tasks/wasm/`, mirroring the firmware's separation of concerns.

**LCD buffer for minifb rendering:** The WASM framebuffer is stored in `LCD_BUFFER`
(static `Mutex<Option<Vec<u8>>>` in `context.rs`). The main thread's render loop checks it
each frame. If `Some`, it renders the raw RGB565 buffer directly. If `None`, it falls back
to rendering the `LcdState` (menu screen). The buffer is cleared when the WASM session
ends via `run_program`'s cleanup.

**Button forwarding during WASM execution:** When the stack top is `HostedApp`, the menu
task forwards button presses as `HostIpcMessage::Wire(HostIpcMessage::HexButton(..))`
using `try_send` (non-blocking). The WASM tick loop peeks this channel via `try_peek()`
and passes the message length to the WASM's `tick()` function, which calls
`extern_read_host_ipc_message` to consume it. This avoids the menu thread blocking if the
WASM is busy.

**Stack events for WASM lifecycle:** Instead of a shared `AppState` RwLock, the WASM
runner signals completion by sending `StackEvent::Popped` through the `StackSignal`.
The menu runner's hosted-app pump (`run_hosted_app` in `app/src/menu/mod.rs`) selects
input against `stack_signal.receive()`; when the boot button is pressed,
`HostIpcMessage::Runtime(HostRuntimeCommand::Stop)` is sent and the pump waits for the
resulting `StackEvent::Popped` before resuming the previous app.

**`wasmi_runner` sends `Stopped` — callers must NOT send it again:** The WASM interpreter
in `app/src/wasm/mod.rs` sends `WasmIpcMessage::Stopped` on natural completion (line 99)
and on abort via Stop message (line 73). The firmware's `wasm_host_loop` was duplicating
this by sending a second `Stopped` after `wasmi_runner` returned, causing two `Popped`
events per WASM session → double-pop of the stack. The fix: firmware `wasm_host_loop`
must NOT send `Stopped` — `wasmi_runner` already handles it.

**Stack-signal consumption:** Each `run_*` handler selects `stack_signal.receive()`
alongside its input source (a single flat `select`, no nesting). The hosted pump
forwards hex buttons with `try_send`; a boot button sends
`HostRuntimeCommand::Stop` and the pump waits for the resulting `StackEvent::Popped`
before resuming the previous entry.

## Key Design Decisions & Crate Boundaries

### Why `embassy_net::Stack` is in firmware, not app

`embassy_net::Stack<'static>` is `!Send + !Sync` (uses `RefCell` internally). It must NOT
appear in the `app` crate because:
- `MenuRunnerContext` needs to be `Send` for desktop's `thread::spawn`
- `Platform` requires `Sync` for `Arc<dyn Platform>` sharing
- Any containing type inherits the `!Send + !Sync`

Instead, `HardwareHttpClient` wraps the stack with `unsafe impl Send + Sync` (same pattern
as `SendFilesystem` — sound because all access is serialized by the async runtime on a
single core). The `HttpClient` trait itself is `Send + Sync`.

### Channel-based Streaming over Callbacks

The `HttpClient` trait uses a shared `Channel` for streaming rather than closures:

```rust
pub trait HttpClient: Send + Sync + fmt::Debug {
  fn request<'a>(
    &'a self,
    req: HttpRequest,
    channel: &'a HttpEventChannel,
  ) -> Pin<Box<dyn Future<Output = ()> + 'a>>;
}
```

**Why not callbacks?** Acallbacks returning `Pin<Box<dyn Future>>` would require boxing
per-callback, which is expensive and complex. A channel is concrete, bounded (capacity 2),
and works with join.

### CPU Parking for Flash Writes

The flash storage handle created in `firmware/src/bin/rustagon.rs` is wrapped
with `multicore_auto_park()` (esp-storage `AutoPark` multicore strategy). That
wrapper is the **single owner of app-core parking for flash I/O**: every flash
write/erase (littlefs, config files, OTA chunks, otadata) parks the app core
for the duration of the operation and unparks it on completion — on every
path, including errors. Do **not** park the app core manually around flash
access: the old `ota_begin()` park-never-unparked bug and the
`CpuGuard`/`CpuControl::steal()` boilerplate were removed in favour of this.

### `CriticalSectionRawMutex` preference

Use `CriticalSectionRawMutex` instead of `NoopRawMutex` for any shared state that crosses
thread or task boundaries. `NoopRawMutex` is `!Sync` and prevents structs from being `Sync`,
which breaks `Arc<dyn Trait>` and `thread::spawn`.

The `StackEventChannel` uses `CriticalSectionRawMutex` for thread-safe
communication between the menu runner and the IPC handler task.

### What Makes a Good Platform Manager

1. **Trait Design** — Minimal interface, async for I/O, single state enum.
2. **Internal Encapsulation** — No channels or sync types leak to the application.
3. **State Notification** — `WatchedValue<T>` for state, `EventQueue<T,N>` for events.
4. **Platform Implementations** — Must be implementable without hardware where possible.
5. **Hardware Implementation** — Uses concrete types from the crate, protected by mutexes.
6. **Object-Safe Traits for Handles** — `Send + Sync + Debug`, `Pin<Box<dyn Future>>` returns.
7. **Handle via Deref** — `StorageHandle` proxies via `Deref` to avoid boilerplate.
8. **Platform is I/O centric** — The platform abstraction (`desktop/src/platform/`,
   `firmware/src/platform/`) is about I/O subsystems (display, input, storage, HTTP).
   WASM runtime concerns live in `tasks/`, not in the platform.

## I2C Bus Topology

One physical I2C bus (ESP32-S3 I2C0, SDA=GPIO45, SCL=GPIO46, 133kHz) split into 8
virtual buses by a **TCA9548A** I2C multiplexer at address `0x77`:

| Port | Mux Bit | Purpose |
|------|---------|---------|
| 0 | `0b00000001` | Top bus (frontboard: touch, IMU, USB-C power) |
| 1 | `0b00000010` | Hexpansion port 1 |
| 2 | `0b00000100` | Hexpansion port 2 |
| 3 | `0b00001000` | Hexpansion port 3 |
| 4 | `0b00010000` | Hexpansion port 4 |
| 5 | `0b00100000` | Hexpansion port 5 |
| 6 | `0b01000000` | Hexpansion port 6 |
| 7 | `0b10000000` | System bus (BQ25895, AW9523B #0/#1/#2, FUSB302B in) |

AW9523B #3 at `0x58` on the top bus (port 0) was added with the 2026 frontboard
for hex buttons.

## Remaining Work

### Medium Priority

1. **Add feature flags to `app` crate** — `#[cfg(feature = "std")]` for desktop-specific
   impls (e.g. sleep via std::thread), `#[cfg(feature = "embassy")]` for embassy.

2. **Remove `esp_hal::system::software_reset()` calls outside Platform** — `wifi_join.rs`
   still calls directly; should use `platform.software_reset()`.

4. **Desktop hexpansion simulation** — The desktop `HexpansionManager` is a no-op stub.
   Implement file-backed simulation for hexpansion EEPROMs and virtual keyboard events.

### Low Priority

1. **Desktop improvements**
   - Render icons (currently empty on non-xtensa)
   - Improve keyboard mappings

2. **Interrupt-driven keyboard** — The TCA8418 driver currently polls every 20ms.
   The HS_H pin on the hexpansion port could be used as an interrupt line for zero-latency
   key event detection.

### Driver Table in `rustagon.rs`

Drivers are registered in a static table in the firmware entry point:

```rust
const DRIVER_TABLE: &[DriverEntry] = &[
    DriverEntry { vid: 0xBAD3, pid: 0x4EEB, factory: tca8418_driver_factory },
];
```

On hexpansion detection, the polling task iterates this table for a VID:PID match.
If found, it constructs a `DeviceIo` with a `DeviceI2c` wrapping a `MaskedI2cBus`
clone for that port, and calls the factory function. The factory spawns an Embassy
task via the provided `Spawner`. Drivers self-terminate after 3 consecutive I2C
errors (hexpansion removed).

### `MaskedI2cBus` REPEATED START handling

The `MaskedI2cBus` must properly handle combined write+read transactions with
REPEATED START (needed for register-based I2C devices like the TCA8418). The
implementation delegates to the underlying ESP HAL's `transaction()` after
setting the mux channel:

```rust
i2c.write(Self::MUX_ADDR, &[self.mux_bits])?;
embedded_hal::i2c::I2c::transaction(&mut *i2c, address, operations)
```

Never iterate operations with separate `write()`/`read()` calls — that produces
STOP between operations instead of REPEATED START, breaking register reads.

## Common Pitfalls & Solutions

### `!Send + !Sync` types in `app` crate

**Problem:** Types like `embassy_net::Stack<'static>` (uses `RefCell`) are `!Send + !Sync`.
If they appear in `app/` types, the `app` crate inherits `embassy-net` dependency, and
structs become unusable from `thread::spawn` (desktop).

**Solution:** Keep hardware-specific types in `firmware/` only. Use `unsafe impl` wrappers
(like `SendFilesystem`, `HardwareHttpClient`) in the firmware crate, never in `app/`.
The `app` crate only sees the `HttpClient` trait + handle, which are `Send + Sync`.

### `Pin<Box<dyn Future>>` methods taking `&str` params

**Problem:** An object-safe trait that returns `Pin<Box<dyn Future + Send + '_>>` cannot
borrow function parameters in the returned future — the future must outlive the call, but
params are dropped when the function returns.

**Solution:** Take owned types (`String`, `Vec<u8>`) instead of references (`&str`, `&[u8]`).
Move them into `async move { }` blocks inside the `Box::pin(...)`.

### RwLock with NoopRawMutex vs CriticalSectionRawMutex

**Problem:** `NoopRawMutex` is `!Sync`. If used in `Arc<RwLock<NoopRawMutex, T>>`, the
`Arc` is `!Send + !Sync`. This breaks desktop's `thread::spawn`.

**Solution:** Use `RwLock<CriticalSectionRawMutex, T>` for any state shared across threads
or passed through `MenuRunnerContext`. Only use `NoopRawMutex` for purely local state
within a single task.

### `SendFilesystem` pattern for `!Send` third-party types

**When to use:** A third-party type is `!Send` but all access is serialized through a
mutex or single-core executor. Wrap it with `unsafe impl Send` and document safety.

```rust
struct SendFilesystem<S: Storage>(Filesystem<S>);
unsafe impl<S: Storage> Send for SendFilesystem<S> {}
impl<S: Storage> Deref for SendFilesystem<S> { type Target = Filesystem<S>; ... }
```

Applied to: `littlefs_rust::Filesystem` (raw C pointers), `embassy_net::Stack` (RefCell).

### Duplicate State Representations

**Problem:** Having both `WifiStatus` and `WifiStatusMessage` with conversion logic.
**Solution:** Single enum, single source of truth.

### `Rgb565::new()` treats args as raw 5/6/5 values

**Problem:** `Rgb565::new(r, g, b)` expects r in 0–31, g in 0–63, b in 0–31.
**Solution:** Construct via `Rgb565::from(Rgb888::new(r, g, b))` for proper conversion.

### Button Debouncing Belongs in the Platform

**Problem:** Mechanical boot button bounce generated multiple `BootButton` events per
physical press. The app level tried to compensate with `try_next_button()` drain loops,
but this is fragile and couples app logic to hardware characteristics.

**Solution:** Debounce at the source — the firmware's `button_monitoring_task` in
`firmware/src/platform/system.rs` waits 50ms after each falling edge before allowing
the next detection. This suppresses contact bounce entirely at the platform level,
keeping the app crate hardware-agnostic.

### `DeviceEvent` channel — single-consumer for driver output

Device drivers (keyboard, future sensors) push `DeviceEvent`s into a shared
`EventQueue<DeviceEvent, 32>` created by the `HardwareHexpansionManager`.
`AppRunContext::next_event()` multiplexes hexpansion + device events so keyboard
input is immediately delivered to the foreground app (inside its `run()` loop)
without needing a badge button press.

**Why a single queue, not per-driver:** A single shared queue is simpler and
sufficient — all device events flow to the same consumers (the menu loop and
apps). Per-driver queues would require multiplexed consumption, adding complexity
without benefit for the current use cases.

**Navigation keys are unified into `HexButton` at the platform boundary.** Keyboard
arrow keys and Enter are translated into `HexButton` presses (Up/Down/Left/Right/Fire)
*below* the app layer, so menu apps and WASM guests cannot tell keyboard input from the
physical badge buttons:
- **Firmware:** the `tca8418` keyboard driver calls `KeyCode::to_hex_button()`
  (`app::types`) and pushes the press/release onto the shared button queue
  (`HardwareInputManager::button_queue()`, plumbed through the driver factory).
  Arrows/Enter never appear as `DeviceEvent::Keyboard`; only character/editing keys
  (letters, digits, Backspace, Delete, Home, End, Tab, Space) do.
- **Desktop:** `desktop/src/main.rs` maps arrows/Enter directly to `HexButton` in
  `key_to_hex_button` and excludes them from `key_to_keycode`, so they are never
  keyboard events there either.

Consequently `nav_button_from_device_event` in `app/src/apps/common.rs` (the single
nav-injection site, used by the hosted-app pump and the root menu) only maps
Escape→HexF and Tab→HexE; arrows/Enter never reach it. Built-in menu apps handle
directions/Fire via `AppRunEvent::Input(AppInput::Button(..))` (the Editor does:
`handle_nav_button`) and still receive Tab/Escape as raw keyboard events. WASM guests
always received only `HexButton` over the wire, so they are unaffected.

**Shift is app-side domain logic.** The platform only reports the raw
`KeyCode::Shift` press/release (the `tca8418` driver maps both shift keys to
`KeyCode::Shift`; `desktop/src/main.rs` maps `LeftShift`/`RightShift`). The app
crate owns shifting via `KeyCode::to_char(shifted)` in `app/src/keys.rs`
(letters uppercase, digits/punctuation shift per `SHIFTED_SYMBOL_MAP`, mirroring
the KeebDeck `app.py`). Apps track shift state from `KeyCode::Shift` press/release
events and pass it in — see `EditorApp::shifted`. Do **not** move the symbol map
into the driver or desktop key mapping; that would duplicate it across two
platforms.

### `AppEvent` — external events for MenuApps

Apps receive external events via `AppRunContext::next_event()` (or `next()`) where
`AppEvent` is:

```rust
pub enum AppEvent {
  Hexpansion(HexpansionEvent),  // plug/unplug
  Device(DeviceEvent),          // keyboard key, future device events
}
```

Apps dispatch on them inside `run()` to react to keyboard input (the Editor app),
hexpansion state changes (the HexpansionViewer app), or future event types without
requiring a badge button press.

### Futures do nothing unless awaited — un-awaited `join!`

**Problem:** `firmware/src/tasks/ipc_handler.rs` built the HTTP-response relay with
`join!(http_client.request(..), async { .. });` but **forgot the `.await`**. The `join!`
future was created and immediately dropped — never polled — so the request future (and
its consumer) never ran. The WASM guest waited forever for a response: "the app does
nothing but it also does not crash" (it just keeps ticking). Desktop was unaffected
because `run_program` uses `futures::future::join(..).await`.

**Solution:** Always `.await` the future produced by `join!`. The compiler warns with
`unused implementer of Future that must be used` / `futures do nothing unless you .await
or poll them` — treat that warning as an error. When adding a guest-request handler,
verify the request future is actually driven (e.g. the `HTTP: request url=` log inside
`perform_http_request_streaming` appears on the serial console — it is at debug level, so
build with `ESP_LOG=DEBUG`).

### `StackSignal` — single-consumer event delivery

**Problem:** Using an embassy-sync `Channel<M, StackEvent, N>` with multiple `Receiver`
handles for IPC handler → menu runner communication allowed the same `StackEvent` to
be consumed by two different receivers on the cooperative single-threaded executor,
causing double-pops of the stack.

**Solution:** `app::menu::state::StackSignal` pairs an `AtomicU8` with an embassy-sync
`Signal<CriticalSectionRawMutex, ()>`. The `send()` method stores the event value via
atomic `store` and wakes the waiter via `signal()`. The `receive()` (async) and
`try_receive()` (sync) methods consume the event via atomic `swap(NONE)` — a single-
consumer operation. No channel, no multiple receivers, no race.

### LCD raw-frame path — the SPI bus is the bottleneck (see `DMA.md`)

**TL;DR:** the fastest WASM→LCD frame path is a single **blocking** SPI write
from core 1, straight from the guest's buffer — no queues, no DMA, no copies.
`DisplayManager::signal_raw_frame` (`app/src/platform/display.rs`) owns the
write on each platform; `HardwareWasmHost`/`DesktopWasmHost` just call it.

- Never busy-wait a blocking SPI flush on core 0: it couples WASM rendering to
  the menu/wifi/net/HTTP/hexpansion tasks and caused ~half-second stalls every
  few seconds.
- Async DMA (`SpiDmaBus::write_async`) was ~5 fps: it pays an interrupt
  round-trip per 32,736-byte chunk (7 command writes + 8 pixel chunks per
  frame). A single contiguous DMA transfer per frame would be the only viable
  async shape.
- The ~87 fps ceiling is the SPI bus itself (115,200 bytes @ 80 MHz ≈ 11.5 ms).
- Full writeup, measurements, and esp-hal API notes: **`DMA.md`** (repo root).
  Read it before touching the display path again.

### `#[serde(default)]` on `String` fields gives `""`, not your intended default

**Problem:** `DeviceConfig` fields added after the initial release need a serde
default so legacy configs still deserialize. A bare `#[serde(default)]` on a
`String` field falls back to `String::default()` (`""`), not the value in
`DeviceConfig::default()` — so a legacy config silently got an empty mDNS
hostname.

**Solution:** Use named default functions that mirror the struct's defaults:
`#[serde(default = "DeviceConfig::default_device_name")]` (see
`app/src/types.rs`). `app/src/types.rs` has a test
(`legacy_minimal_config_deserializes`) proving a pre-`device_name` config
deserialises with the intended values — keep it in mind when adding fields.

## Related Files

- **Platform trait:** `app/src/platform/traits.rs`
- **Hexpansion manager trait:** `app/src/platform/hexpansion.rs`
- **HTTP client trait:** `app/src/platform/http.rs`
- **TCP client:** `app/src/platform/tcp.rs` (session model), `firmware/src/platform/tcp.rs` (pump task ownership), `desktop/src/platform/tcp.rs` (thread-per-connection)
- **Menu system:** `app/src/menu/mod.rs` (menu loop + `RootMenuApp` + `run_*` entry functions)
- **Menu state/stack:** `app/src/menu/state.rs`
- **Menu types:** `app/src/menu/types.rs`
- **App run-loop contract:** `app/src/apps/common.rs` (`MenuApp`, `AppRunContext`, `AppRunEvent`, `nav_button_from_device_event`)
- **AppSpawner:** `app/src/platform/spawner.rs`, firmware `firmware/src/platform/spawner.rs`, desktop `desktop/src/platform/common.rs`
- **Menu apps:** `app/src/apps/mod.rs` (enum + list/load), `app/src/apps/*.rs`
- **UI widget toolkit:** `app/src/ui/` (`text_input`, `list`, `form`, `terminal`, `progress`) — the shared building blocks the Editor, SSH connect form, root menu, OTA/App Store downloads build on
- **SSH engine:** `app/src/ssh/mod.rs` (`SshSession`, incl. `server_host_key()` for TOFU fingerprinting), SSH key → byte mapping in `app/src/ssh/keys.rs`, terminal widget in `app/src/ui/terminal.rs`
- **KV store:** `app/src/kv.rs` (`KvNamespace` over `StorageHandle`), accessor on `MenuAppContext`
- **Protocol types:** `app/src/protocol.rs` (runtime supersets), `libs/wasm_protocol/src/lib.rs` (wire types), `sdk/src/protocol.rs` (SDK re-export + host functions)
- **Domain types:** `app/src/types.rs`
- **Firmware entry point:** `firmware/src/bin/rustagon.rs`
- **Firmware platform:** `firmware/src/platform/hardware.rs`
- **Firmware hexpansion:** `firmware/src/platform/hexpansion.rs`
- **Device drivers:** `firmware/src/platform/drivers/mod.rs` + `tca8418.rs`
- **Firmware HTTP client:** `firmware/src/platform/http.rs`
- **I2C infrastructure:** `firmware/src/utils/i2c.rs`
- **IPC handler (firmware):** `firmware/src/tasks/ipc_handler.rs`
- **Menu wrapper (firmware):** `firmware/src/tasks/menu/mod.rs`
- **Desktop platform:** `desktop/src/platform/mod.rs`
- **WASM IPC:** `firmware/src/tasks/wasm/`
- **Desktop WASM runner:** `desktop/src/tasks/wasm/`
- **Network infrastructure:** `firmware/src/tasks/net.rs`
- **Power manager (firmware):** `firmware/src/platform/power.rs` (work loop + `WatchedValue<PowerStatus>`)
- **WatchedValue / EventQueue:** `app/src/utils/sync.rs` (`app::utils::{WatchedValue, EventQueue}`)
- **ESP32-S3 flash driver:** `libs/esp32s3_embedded_tools/src/flash.rs`
- **Generic LocalFs:** `libs/embedded_tools/src/local_fs.rs`
- **Headless testing:** `app/src/testing/` (`MockPlatform` + `AppDriver`, `testing` feature), golden tests + fixtures in `app/tests/golden.rs` + `app/tests/golden/`, KV round-trip tests in `app/tests/kv.rs`, push/result flow tests in `app/tests/menu_results.rs`
