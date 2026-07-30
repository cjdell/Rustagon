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

Used by `HardwareSystemManager` (boot button), `HardwareInputManager` (hex buttons),
and `HardwareHexpansionManager` (device events from drivers).

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
│   │   ├── common.rs                 #   MenuApp trait (init/handle_input/render, handle_event), MenuAppContext<P>, AppAction
│   │   ├── app_store.rs              #   AppStoreApp<P>
│   │   ├── config.rs                 #   ConfigApp<P>
│   │   ├── editor.rs                 #   EditorApp<P> (text editor, consumes DeviceEvent::Keyboard)
│   │   ├── files.rs                  #   FilesApp<P>
│   │   ├── hexpansion_viewer.rs      #   HexpansionViewerApp<P> (shows slot state)
│   │   ├── input_test.rs             #   InputTestApp<P>
│   │   ├── mod.rs                    #   MenuAppType<P> enum + list/load functions
│   │   ├── ota_updater.rs            #   OtaUpdaterApp<P>
│   │   ├── power_info.rs             #   PowerInfoApp<P>
│   │   └── wifi_scanner.rs           #   WifiScannerApp<P>
│   ├── menu/                         # Full menu system (async fn, not Embassy task)
│   │   ├── mod.rs                    #   menu_task<P: Platform>() — the main loop
│   │   ├── state.rs                  #   AppStackEntry<P>, StackEntryType, StackEvent, AppStack types
│   │   ├── execute.rs                #   MenuState::execute_option
│   │   ├── types.rs                  #   MenuRunnerContext<P>, MenuContext<P>, AppLoader<P>
│   │   └── menus.rs                  #   MenuProvider trait, StaticMenu
│   ├── native/                       # Native app types (stub, empty in app crate)
│   ├── platform/                     # Platform trait + 10 handle/manager pairs + HttpClient
│   │   ├── traits.rs                 #   Platform trait (17 methods incl. hexpansion_manager)
│   │   ├── display.rs                #   DisplayManager trait + DisplayHandle
│   │   ├── hexpansion.rs             #   HexpansionManager trait + HexpansionHandle + DeviceIo + DeviceI2c
│   │   ├── http.rs                   #   HttpClient trait + HttpClientHandle + HttpEventChannel
│   │   ├── input.rs                  #   InputManager trait + InputHandle
│   │   ├── led.rs                    #   LedManager trait + LedHandle + LedError
│   │   ├── power.rs                  #   PowerManager trait + PowerHandle
│   │   ├── storage.rs                #   StorageHandle, ConfigHandle<State>
│   │   ├── system.rs                 #   SystemManager trait + SystemHandle
│   │   └── wifi.rs                   #   WiFiManager trait + WiFiHandle + WifiStatus
│   ├── protocol.rs                   # HttpRequest, WasmIpcMessage, HostIpcMessage, channels
│   ├── types.rs                      # Domain types (HexButton, LedRequest, DeviceConfig,
│   │                                  #   HexpansionInfo, HexpansionEvent, DeviceEvent,
│   │                                  #   KeyboardEvent, KeyCode, etc.)
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

### Build

```sh
cd firmware && cargo build -r --bin rustagon    # ESP32-S3
cd desktop && cargo build                       # macOS/linux (minifb window)
```

## How the Menu Moves Through Crates

1. `app::menu::menu_task<P: Platform>()` — the full menu system. Uses an **AppStack** to
   support multitasking: apps can launch sub-apps (WASM, native, or other menu apps), and
   the parent stays on the stack with its state preserved. Generic over any platform.
2. `firmware::tasks::menu::menu_task` — thin Embassy wrapper (~20 lines). Constructs
   `app::menu::types::MenuRunnerContext<HardwarePlatform>` and calls `app::menu::menu_task()`.
3. `desktop/src/main.rs` — calls `app::menu::menu_task()` directly via `thread::spawn`.

### AppStack Architecture

The menu maintains a `Vec<AppStackEntry<P>>` where each entry is one of:
- **RootMenu** — the main navigation list (always at the bottom)
- **MenuApp(app)** — a built-in app (Files, Config, etc.) with its mutable state
- **HostedApp** — a WASM or native app running externally on the second core

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

### App State Machines (not event loops)

Apps no longer own the event loop. Instead they implement the `MenuApp` trait:

```rust
pub trait MenuApp {
  fn render(&self) -> LcdScreen;
  async fn init(&mut self);
  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction;
  /// Called when external events occur (hexpansion plug/unplug, keyboard input)
  /// while the app is foregrounded. Default no-op.
  async fn handle_event(&mut self, event: AppEvent) {}
}
```

The menu runner calls `handle_input()` for each button press and dispatches on the
returned `AppAction` (`Continue`, `Stop`, `LaunchWasm(name)`, `LaunchNative(name)`).
This eliminates the need for global flags (`WASM_LAUNCHING` was removed) and lets the
stack naturally manage the multitasking flow.

**External events via `handle_event`:** The menu loop now `select`s on four event
sources: system button, hex button, stack signal, and a **nested select** of
hexpansion events + device events. When a device event (keyboard key) or hexpansion
event arrives, the foreground app's `handle_event()` is called. This lets apps react
to keyboard input, hexpansion plug/unplug, and future device events without the user
pressing a badge button. The event that triggered the select is passed to the app,
followed by a non-blocking drain of any remaining queued events.

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
task forwards button presses as `HostIpcMessage::HexButton` using `try_send` (non-blocking).
The WASM tick loop peeks this channel via `try_peek()` and passes the message length to
the WASM's `tick()` function, which calls `extern_read_host_ipc_message` to consume it.
This avoids the menu thread blocking if the WASM is busy.

**Stack events for WASM lifecycle:** Instead of a shared `AppState` RwLock, the WASM
runner signals completion by sending `StackEvent::Popped` through the `StackSignal`.
The menu runner's `handle_hosted_app` loop waits on `select3(system_button, hex_button, stack_signal.receive())`.
When the boot button is pressed, `HostIpcMessage::Stop` is sent and the menu waits for the
resulting `StackEvent::Popped` before resuming the previous app.

**`wasmi_runner` sends `Stopped` — callers must NOT send it again:** The WASM interpreter
in `app/src/wasm/mod.rs` sends `WasmIpcMessage::Stopped` on natural completion (line 99)
and on abort via Stop message (line 73). The firmware's `wasm_host_loop` was duplicating
this by sending a second `Stopped` after `wasmi_runner` returned, causing two `Popped`
events per WASM session → double-pop of the stack. The fix: firmware `wasm_host_loop`
must NOT send `Stopped` — `wasmi_runner` already handles it.

**`select3` for live handlers, `select2` + post-check for others:** Handlers that need to
react to stack events immediately (`handle_hosted_app`, `handle_menu_app`) use `select3`
that includes `stack_signal.receive()` as a third future. The root menu handler uses
`select2` and checks `stack_signal.try_receive()` after each input — it does not need
immediate WASM-exit response. This avoids the complexity of three-way select in the
simple root-menu case.

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

4. **Desktop hexpansion simulation** — The desktop `HexpansionManager` is a no-op stub.
   Implement file-backed simulation for hexpansion EEPROMs and virtual keyboard events.

### Low Priority

1. **Desktop improvements**
   - Render icons (currently empty on non-xtensa)
   - Improve keyboard mappings

2. **Add unit tests to `app` crate** — Use `MockPlatform` for deterministic tests.

3. **Add `#[serde(default)]` to new `DeviceConfig` fields** — Any field added after
   initial release needs this to avoid breaking deserialization of existing configs.

4. **Interrupt-driven keyboard** — The TCA8418 driver currently polls every 20ms.
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
`EventQueue<DeviceEvent, 32>` created by the `HardwareHexpansionManager`. The
menu loop's `select3`/`select4` includes a `select(hexpansion_event, device_event)`
branch so that keyboard input is immediately delivered to the foreground app without
needing a badge button press.

**Why a single queue, not per-driver:** A single shared queue is simpler and
sufficient — all device events flow to the same consumers (the menu loop and
apps). Per-driver queues would require multiplexed consumption, adding complexity
without benefit for the current use cases.

`KeyboardEvent` navigation keys (arrows, Enter, Escape) are also injected as
`HexButton` events into the input manager at the root menu and for hosted
(WASM/native) apps, so the keyboard works for menu navigation without app changes.

### `AppEvent` — external events for MenuApps

Apps receive external events via `handle_event(&mut self, event: AppEvent)` where
`AppEvent` is:

```rust
pub enum AppEvent {
  Hexpansion(HexpansionEvent),  // plug/unplug
  Device(DeviceEvent),          // keyboard key, future device events
}
```

The default implementation is a no-op. Apps override it to react to keyboard input
(the Editor app), hexpansion state changes (the HexpansionViewer app), or future
event types without requiring a badge button press.

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

## Related Files

- **Platform trait:** `app/src/platform/traits.rs`
- **Hexpansion manager trait:** `app/src/platform/hexpansion.rs`
- **HTTP client trait:** `app/src/platform/http.rs`
- **Menu system:** `app/src/menu/mod.rs`
- **Menu state/stack:** `app/src/menu/state.rs`
- **Menu types:** `app/src/menu/types.rs`
- **Menu apps:** `app/src/apps/mod.rs` (enum + list/load), `app/src/apps/*.rs`
- **Protocol types:** `app/src/protocol.rs`
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
- **WatchedValue:** `firmware/src/utils/watched_value.rs`
- **EventQueue:** `firmware/src/utils/event_queue.rs`
- **ESP32-S3 flash driver:** `libs/esp32s3_embedded_tools/src/flash.rs`
- **Generic LocalFs:** `libs/embedded_tools/src/local_fs.rs`
