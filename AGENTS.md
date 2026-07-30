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
```

### Check the firmware compiles:

```sh
cd firmware && cargo build -r --bin rustagon
```

### Check the desktop compiles:

```sh
cd desktop && cargo build
```

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
  fn storage_manager(&self) -> StorageHandle;
  fn config_manager(&self) -> ConfigHandle<DeviceConfig>;
  async fn format_storage(&self) -> Result<(), FsError>;
  async fn software_reset(&self);
  async fn ota_begin(&self) -> Result<u32, OtaError>;
  async fn ota_write_chunk(&self, offset: u32, data: &[u8]) -> Result<(), OtaError>;
  async fn ota_commit(&self) -> Result<(), OtaError>;
}
```

Each manager returns a cloneable `Handle` that wraps an `Arc<dyn ManagerTrait>`.

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
async runtime).

**Desktop:** implemented via `ureq` (blocking HTTP in a background thread).

### Completed Subsystems

#### 1. LED Management (`app/src/platform/led/`)

**Structure:**
- `traits.rs` - `LedManager` trait with `request(LedRequest)` method
- Hardware impls in `firmware/src/platform/led.rs`, mock in `desktop/`

**Design Pattern:**
- Manager spawns its own background work loop via Embassy task
- Internal channel for requests (completely encapsulated)
- No external channel management needed

#### 2. Power Management (`app/src/platform/power/`)

**Structure:**
- `traits.rs` - `PowerManager` trait with `get_status()` and `power_off()` async methods
- Hardware impl wraps BQ25895 charger IC in `firmware/`

**Design Pattern:**
- Async trait with `Pin<Box<dyn Future>>` for dyn compatibility
- Internal `Arc<RwLock<CriticalSectionRawMutex, T>>` for shared access to I2C device
- Prefer `RwLock` with `CriticalSectionRawMutex` for all internal synchronization

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
matters (button presses, IRQ notifications), use `EventQueue<T, N>` from
`firmware/src/utils/event_queue.rs`.

- Owns its `Channel` behind an `Arc` - no `static` channel and no `Sender`/`Receiver`
  plumbed through `main`; the manager creates the queue in `new()` and clones it into
  its spawned task.
- Consumer: `queue.next().await` - suspends until an event arrives, never polls.
- Producer: `push().await` (backpressure) or `try_push()` (lossy, safe from sync code).
- Cloneable, so it works naturally with `Arc<dyn Trait>` handles.

Used by `HardwareSystemManager` (boot button) and `HardwareInputManager` (hex buttons).

**Never write a `loop { check_shared_vec(); Timer::after(..).await }` in a manager** -
that both adds latency and hogs locks. Use `EventQueue` (events) or `WatchedValue`
(state) instead.

## Current Crate Architecture

```
rustagon/
├── app/                              # Platform-agnostic application library (no_std)
│   ├── apps/                         # All menu apps (generic over P: Platform)
│   │   ├── common.rs                 #   MenuAppAsync trait, MenuAppContext<P>, WASM_LAUNCHING
│   │   ├── app_store.rs              #   AppStoreApp<P>
│   │   ├── config.rs                 #   ConfigApp<P>
│   │   ├── files.rs                  #   FilesApp<P>
│   │   ├── input_test.rs             #   InputTestApp<P>
│   │   ├── mod.rs                    #   MenuAppType<P> enum + list/load functions
│   │   ├── ota_updater.rs            #   OtaUpdaterApp<P>
│   │   ├── power_info.rs             #   PowerInfoApp<P>
│   │   └── wifi_scanner.rs           #   WifiScannerApp<P>
│   ├── menu/                         # Full menu system (async fn, not Embassy task)
│   │   ├── mod.rs                    #   menu_task<P: Platform>() — the main loop
│   │   ├── state.rs                  #   MenuState<P>, AppState enum
│   │   ├── execute.rs                #   MenuState::execute_option
│   │   ├── types.rs                  #   MenuRunnerContext<P>, MenuContext<P>, AppLoader<P>
│   │   └── menus.rs                  #   MenuProvider trait, StaticMenu
│   ├── native/                       # Native app types (stub, empty in app crate)
│   ├── platform/                     # Platform trait + 9 handle/manager pairs + HttpClient
│   │   ├── traits.rs                 #   Platform trait (16 methods)
│   │   ├── display.rs                #   DisplayManager trait + DisplayHandle
│   │   ├── http.rs                   #   HttpClient trait + HttpClientHandle + HttpEventChannel
│   │   ├── input.rs                  #   InputManager trait + InputHandle
│   │   ├── led.rs                    #   LedManager trait + LedHandle + LedError
│   │   ├── power.rs                  #   PowerManager trait + PowerHandle
│   │   ├── storage.rs                #   StorageHandle, ConfigHandle<State>
│   │   ├── system.rs                 #   SystemManager trait + SystemHandle
│   │   └── wifi.rs                   #   WiFiManager trait + WiFiHandle + WifiStatus
│   ├── protocol.rs                   # HttpRequest, WasmIpcMessage, HostIpcMessage, channels
│   ├── types.rs                      # Domain types (HexButton, LedRequest, DeviceConfig, etc.)
│   └── utils.rs                      # Sleep helper only
├── desktop/                          # Desktop platform (std, minifb, ureq)
│   └── src/
│       ├── main.rs                   # Minifb window, menu_task on bg thread
│       └── platform/                 # DesktopPlatform, DesktopHttpClient (ureq), mocks
├── drivers/                          # Hardware driver IC crates
│   ├── bq25895/                      # BQ25895 charger IC driver
│   └── cy8cmbr3116/                  # CY8CMBR3116 capacitive touch controller driver
├── firmware/                         # ESP32-S3 host (thin platform impl + tasks)
│   ├── src/
│   │   ├── bin/rustagon.rs           # Entry point, hardware init, task spawning
│   │   ├── platform/                 # HardwarePlatform + manager impls
│   │   │   ├── hardware.rs           #   HardwarePlatform struct + Platform impl
│   │   │   ├── http.rs               #   HardwareHttpClient (wraps reqwless)
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
│   ├── emulator/                     # Legacy WASM emulator
│   ├── esp32s3_embedded_tools/       # ESP32-S3 flash driver
│   └── procmacros/                   # include_rgb565_icon!, partition_offset! macros
├── sdk/                              # WASM SDK for emulator programs
├── uploader/                         # Firmware upload tool
└── Cargo.toml                        # Workspace root

### What Must NOT Be In Each Crate

app/:
  ✗ embassy-net (Stack is !Send + !Sync — must stay in firmware)
  ✗ esp-hal, esp-alloc, esp-storage
  ✗ ureq, reqwless, or any HTTP client library
  ✗ Hardware-specific types (I2C, GPIO, SPI, etc.)

firmware/:
  ✗ Menu logic (navigation, app listing, button handling)
  ✗ App domain code (AppStore logic, download, OTA)
  ✓ Thin Embassy task wrappers that call into app::

desktop/:
  ✗ Menu logic
  ✗ App domain code
  ✓ Platform impl (DesktopPlatform, DesktopHttpClient)
```

### Build

```sh
cd firmware && cargo build -r --bin rustagon    # ESP32-S3
cd desktop && cargo build                       # macOS/linux (minifb window)
```

## How the Menu Moves Through Crates

1. `app::menu::menu_task<P: Platform>()` — the full menu system. Handles button input,
   app launching, WASM_LAUNCHING flag, AppState transitions. Generic over any platform.
2. `firmware::tasks::menu::menu_task` — thin Embassy wrapper (~20 lines). Constructs
   `app::menu::types::MenuRunnerContext<HardwarePlatform>` and calls `app::menu::menu_task()`.
3. `desktop/src/main.rs` — calls `app::menu::menu_task()` directly via `thread::spawn`.

The IPC handler (`firmware::tasks::ipc_handler`) runs as a separate Embassy task. It
shares `Arc<RwLock<CriticalSectionRawMutex, AppState>>` with the menu task for coordinating
WASM app lifecycle (Started/Stopped transitions, WASM_LAUNCHING flag).

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

### `CriticalSectionRawMutex` preference

Use `CriticalSectionRawMutex` instead of `NoopRawMutex` for any shared state that crosses
thread or task boundaries. `NoopRawMutex` is `!Sync` and prevents structs from being `Sync`,
which breaks `Arc<dyn Trait>` and `thread::spawn`.

Both `app_state` in `MenuRunnerContext` and the `app` field in `MenuState` use
`RwLock<CriticalSectionRawMutex, AppState>`.

### What Makes a Good Platform Manager

1. **Trait Design** — Minimal interface, async for I/O, single state enum.
2. **Internal Encapsulation** — No channels or sync types leak to the application.
3. **State Notification** — `WatchedValue<T>` for state, `EventQueue<T,N>` for events.
4. **Mock Implementations** — Must be implementable without hardware.
5. **Hardware Implementation** — Uses concrete types from the crate, protected by mutexes.
6. **Object-Safe Traits for Handles** — `Send + Sync + Debug`, `Pin<Box<dyn Future>>` returns.
7. **Handle via Deref** — `StorageHandle` proxies via `Deref` to avoid boilerplate.

## Remaining Work

### High Priority

1. **HTTP headers support** — `perform_http_request_streaming` doesn't send request headers.
   The `HttpRequest` has `headers` but the firmware impl ignores them.

2. **Move `WatchedValue` and `EventQueue` into `app` crate** — Currently in `firmware/src/utils/`,
   both are platform-agnostic (only use embassy-sync). Desktop WiFi/Input managers need them.

3. **Power Status Polling into Manager** — Similar to LED's work loop pattern.
   Would remove `power_monitoring_task` from `i2c.rs`.

4. **OTA CPU control** — `ota_begin()` parks the second core via `CpuControl::steal()`.
   This works but the `CpuControl` ownership is not tracked (uses unsafe + steal).
   Consider storing CpuControl in HardwarePlatform for cleaner lifecycle.

### Medium Priority

1. **Add feature flags to `app` crate** — `#[cfg(feature = "std")]` for desktop-specific
   impls (e.g. sleep via std::thread), `#[cfg(feature = "embassy")]` for embassy.

2. **Clean up warnings** — Many unused imports from the refactoring.

3. **Remove `esp_hal::system::software_reset()` calls outside Platform** — `wifi_join.rs`
   still calls directly; should use `platform.software_reset()`.

### Low Priority

1. **Desktop improvements**
   - Render icons (currently empty on non-xtensa)
   - Add WASM stubs so FilesApp "Execute" shows a message
   - Improve keyboard mappings

2. **Add unit tests to `app` crate** — Use `MockPlatform` for deterministic tests.

3. **Add `#[serde(default)]` to new `DeviceConfig` fields** — Any field added after
   initial release needs this to avoid breaking deserialization of existing configs.

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

## Related Files

- **Platform trait:** `app/src/platform/traits.rs`
- **HTTP client trait:** `app/src/platform/http.rs`
- **Menu system:** `app/src/menu/mod.rs`
- **Menu apps:** `app/src/apps/mod.rs` (enum + list/load), `app/src/apps/*.rs`
- **Protocol types:** `app/src/protocol.rs`
- **Domain types:** `app/src/types.rs`
- **Firmware entry point:** `firmware/src/bin/rustagon.rs`
- **Firmware platform:** `firmware/src/platform/hardware.rs`
- **Firmware HTTP client:** `firmware/src/platform/http.rs`
- **IPC handler (firmware):** `firmware/src/tasks/ipc_handler.rs`
- **Menu wrapper (firmware):** `firmware/src/tasks/menu/mod.rs`
- **Desktop platform:** `desktop/src/platform/mod.rs`
- **WASM IPC:** `firmware/src/tasks/wasm/`
- **Network infrastructure:** `firmware/src/tasks/net.rs`
- **WatchedValue:** `firmware/src/utils/watched_value.rs`
- **EventQueue:** `firmware/src/utils/event_queue.rs`
- **ESP32-S3 flash driver:** `libs/esp32s3_embedded_tools/src/flash.rs`
- **Generic LocalFs:** `libs/embedded_tools/src/local_fs.rs`
