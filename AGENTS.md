# Architecture & Development Notes for Agents

## Current Architecture

The firmware is structured around a **Platform abstraction** that provides trait-based access to hardware subsystems. This allows the application code to be independent of specific hardware implementations and enables testing with mock implementations.

### Check the firmware compiles:

cd firmware
cargo build -r --bin rustagon

### Platform Trait

The `Platform` trait is the central abstraction in `firmware/src/platform/traits.rs`:

```rust
pub trait Platform: Clone + Send + Sync + Debug {
  fn led(&self) -> LedHandle;
  fn power(&self) -> PowerHandle;
  // Future managers will be added here
}
```

Each manager returns a cloneable `Handle` that wraps an `Arc<dyn ManagerTrait>`.

### Completed Subsystems

#### 1. LED Management (`firmware/src/platform/led/`)

**Structure:**
- `traits.rs` - `LedManager` trait with `request(LedRequest)` method
- `hardware.rs` - `HardwareLedManager` implements LED control via ESP32
- `mock.rs` - `MockLedManager` for testing without hardware
- `effects.rs` - All LED effect types (Rainbow, Breathe, Chase, etc.)

**Design Pattern:**
- Manager spawns its own background work loop via Embassy task
- Internal channel for requests (completely encapsulated)
- No external channel management needed

**Key Learning:** Moving the work loop into the manager itself (rather than keeping it in the tasks module) keeps concerns localized and simplifies the main application flow.

#### 2. Power Management (`firmware/src/platform/power/`)

**Structure:**
- `traits.rs` - `PowerManager` trait with `get_status()` and `power_off()` async methods
- `hardware.rs` - `HardwarePowerManager` wraps BQ25895 charger IC
- `mock.rs` - `MockPowerManager` for testing
- Status polling task remains in `firmware/src/tasks/i2c.rs` (not yet moved into manager)

**Design Pattern:**
- Async trait with `Pin<Box<dyn Future>>` for dyn compatibility
- Internal `Arc<RwLock<CriticalSectionRawMutex, T>>` for shared access to I2C device
- Power control requests come directly through the manager (no external channel)

**Key Learning:** Use `RwLock` with `CriticalSectionRawMutex` for internal synchronization - always `Sync`, no unsafe impls needed.

#### 3. WiFi Management (`firmware/src/platform/wifi/`)

**Structure:**
- `traits.rs` - `WiFiManager` trait with scan, connect, status, and stats methods
- `hardware.rs` - `HardwareWifiManager` implements ESP32 WiFi connection logic
- `mock.rs` - `MockWifiManager` for testing
- Connection task spawned by manager (`spawn_connection_task`)
- `wifi_monitor_task` in `firmware/src/tasks/wifi_monitor.rs` - consumes status updates from manager

**Design Pattern - WatchedValue for State:**
- **Single source of truth:** Custom `WatchedValue<WifiStatus>` primitive stores current status
- Provides async reads via `get()`, awaitable changes via `wait_for_change()`, and updates via `set()`
- No external receiver management - all encapsulated in the manager
- Stats tracked via `Arc<AtomicU32>` for connection attempts and successes

**Key Methods:**
- `get_status()` - Get current status (async, returns pinned future)
- `wait_for_status_change()` - Wait for next status change (async, returns pinned future)
- `scan()` - Perform WiFi network scan
- `set_desired_state()` - Request Online/Offline (manager handles reconnection logic)
- `get_stats()` - Get connection attempt statistics

**WatchedValue Pattern Benefits:**
- Single source of truth - no duplication of state
- Multiple independent waiters can await changes simultaneously
- Cloneable - passed through Platform trait naturally
- Async-safe - works in all Embassy async contexts
- No external receiver management needed
- Clean, simple API: `get()`, `set()`, `wait_for_change()`

**Integration with Monitor Task:**
- Monitor task calls `platform.wifi_manager().wait_for_status_change().await` in loop
- Updates LCD display and LED colors based on WiFi state
- No additional channels or receivers needed - everything through Platform trait
- Only the Platform object needed for all WiFi control and observation

### State Management Pattern

The WiFi implementation demonstrates the **preferred pattern for manager state notifications**:

**Pattern Evolution:**
1. **Anti-pattern (generation counters)** - Polling with atomic counters, missed updates
2. **Intermediate (Embassy watch)** - Better but required external receiver management
3. **Final (WatchedValue primitive)** - Clean, complete, encapsulated

**WatchedValue Pattern:**
- Custom primitive combining RwLock for state + Signal for notifications
- Single API: `get()`, `set()`, `wait_for_change()`
- Everything internal to manager - no leaking types
- Cloneable for trait object compatibility

**This pattern should be used for any manager that needs to store state and notify interested parties of changes.**

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
├── app/                    # Platform-agnostic application library (no_std)
│   ├── apps/               # Menu apps (Config, Files, WiFiScanner — generic over P: Platform)
│   │   ├── common.rs       #   MenuAppAsync trait, MenuAppContext<P>, WASM_LAUNCHING flag
│   │   ├── config.rs       #   ConfigApp<P>
│   │   ├── files.rs        #   FilesApp<P>
│   │   └── wifi_scanner.rs #   WifiScannerApp<P>
│   ├── menu/               # Generic menu system (async fn, not Embassy task)
│   │   ├── mod.rs          #   menu_task<P: Platform>()
│   │   ├── state.rs        #   MenuState<P>
│   │   ├── execute.rs      #   MenuState::execute_option
│   │   ├── types.rs        #   MenuRunnerContext<P>, MenuContext<P>
│   │   └── menus.rs        #   MenuProvider trait, StaticMenu
│   ├── native/             # Native app types (stub, firmware-specific apps live in firmware)
│   ├── platform/           # Platform trait + 8 handle/manager pairs
│   │   ├── traits.rs       #   Platform trait (9 methods)
│   │   ├── display.rs      #   DisplayManager trait + DisplayHandle
│   │   ├── input.rs        #   InputManager trait + InputHandle
│   │   ├── led.rs          #   LedManager trait + LedHandle + LedError
│   │   ├── power.rs        #   PowerManager trait + PowerHandle
│   │   ├── wifi.rs         #   WiFiManager trait + WiFiHandle + WifiStatus
│   │   ├── system.rs       #   SystemManager trait + SystemHandle
│   │   └── storage.rs      #   StorageHandle, ConfigHandle<State>
│   ├── protocol.rs         # WasmIpcMessage, HostIpcMessage, HttpRequest, channel types
│   ├── types.rs            # Domain types (HexButton, LedRequest, DeviceConfig, etc.)
│   └── utils/              # Sleep helper only
├── desktop/                # Desktop platform (std, minifb)
│   └── src/
│       ├── main.rs         #   Minifb window, menu_task on bg thread, DesktopFrameBuffer
│       └── platform/       #   DesktopPlatform, DesktopDisplayManager, DesktopInputManager
├── display_renderer/       # LcdState, FrameBuffer trait, icon drawing, menu/notification rendering
├── display_types/          # LcdScreen, Icon20/40, MenuLine, Image types (no_std, serde)
├── firmware/               # ESP32-S3 specific implementation
│   ├── src/
│   │   ├── bin/rustagon.rs # Entry point, hardware init, task spawning
│   │   ├── platform/       # HardwarePlatform + flat manager files (display.rs, input.rs, etc.)
│   │   ├── tasks/          # WASM runtime, HTTP server, wifi_monitor, menu (ESP32-specific)
│   │   └── utils/          # ESP32-specific (LedService, MaskedI2cBus, etc.)
│   └── Cargo.toml
├── embedded_tools/         # LocalFsTrait, ConfigFileTrait (no_std, littlefs-backed)
├── procmacros/             # include_rgb565_icon!, partition_offset!, partition_size! macros
├── emulator/               # Legacy WASM emulator (separate from the new desktop crate)
└── Cargo.toml              # Workspace root

### Build

cd firmware && cargo build -r --bin rustagon    # ESP32-S3
cd desktop && cargo build                       # macOS/linux (minifb window)


### What Makes a Good Platform Manager

Based on LED, Power, and WiFi implementations:

1. **Trait Design**
   - Minimal interface - only operations the app actually needs
   - Async methods when they do I/O or take significant time
   - Return types that can be stored (hence `Pin<Box<dyn Future>>` for async)
   - Single enum for state (e.g., `WifiStatus` not both `WifiStatus` and `WifiStatusMessage`)

2. **Internal Encapsulation**
   - All synchronization primitives internal to the manager
   - No channel or watch types leak to the application
   - Work loops spawned by the manager if needed (like LED)
   - Use Embassy `watch` pattern for state that needs notifications

3. **State Notification Pattern (when needed)**
   - Use `WatchedValue<T>` primitive for managers with state that needs notifications
   - `WatchedValue` combines state storage, async reads, and change notifications
   - API: `get()` for reads, `set()` for updates, `wait_for_change()` for subscribers
   - No external receiver/channel management needed - everything encapsulated
   - Multiple independent waiters can subscribe simultaneously
   - See `firmware/src/utils/watched_value.rs` for implementation
   - Cloneable and works naturally with trait objects

4. **Mock Implementations**
   - Must be implementable without any hardware
   - Should return reasonable default values for testing
   - Excellent for integration tests without an ESP32

5. **Hardware Implementation**
   - Uses concrete types from the crate (I2C, GPIO, etc.)
   - Protected by mutexes for safe concurrent access
   - Prefer `CriticalSectionRawMutex` over `NoopRawMutex` to avoid unsafe impls

6. **Object-Safe Traits for Handles**
   - When a trait will be used as `Arc<dyn Trait>` in a handle, make it object-safe:
     - No `Clone` supertrait (handles provide Clone via `Arc`)
     - No `impl Future` returns — use `Pin<Box<dyn Future + Send + '_>>`
     - Mark `Send + Sync + Debug` for `Arc<dyn Trait>` compatibility
   - The trait can still be used as a generic bound too (e.g., `LocalFsTrait` is both
     object-safe and usable as `T: LocalFsTrait`)

7. **Creating Helper Primitives**
   - Don't be afraid to suggest or create new helper types when a pattern emerges
   - If an async pattern keeps getting duplicated, extract it into a utility primitive
   - Example: `WatchedValue<T>` was created when multiple managers needed:
     - Synchronous reads of mutable state
     - Awaitable notifications of changes
     - Clean API hiding synchronization details
   - Good primitives:
     - Are generic and reusable across multiple managers
     - Hide synchronization complexity with simple APIs
     - Eliminate duplication of similar patterns
     - Work naturally with async/await and trait objects
   - This approach keeps manager code clean and maintainable

## Remaining Work

### High Priority (Unblock `app` crate usage from desktop)

1. **Move `WatchedValue` and `EventQueue` into `app` crate**
   - Currently in `firmware/src/utils/` — used by `WiFiManager` and `InputManager` implementations
   - Both are platform-agnostic (no ESP32 HAL, only embassy-sync)
   - Blocking desktop from implementing WiFi and Input managers that use the same patterns

2. **Abstract HTTP client for AppStore / OTA apps**
   - `AppStoreApp` and `OtaUpdaterApp` remain in `firmware/src/apps/` because they depend on
     ESP32-specific HTTP functions (`perform_http_request` in `utils/http.rs`)
   - Need an `HttpClient` trait in `app::platform` so both firmware (reqwless) and desktop
     (reqwest) can implement it
   - AppStore and OTA can then move into `app` crate and be generic over `P: Platform`

3. **Make firmware's `tasks/menu/` use `app::menu::menu_task()`**
   - Firmware still has its own `tasks/menu/mod.rs` with hardcoded WASM IPC and HTTP event handling
   - Should use `app::menu::menu_task()` and add WASM/HTTP event sources via the Platform trait
     or a separate abstraction
   - This is the last major piece coupling the menu system to ESP32

4. **Add `WASM_LAUNCHING` handling to `app::menu::menu_task()`**
   - Currently `WASM_LAUNCHING` atomic flag is in `app::apps::common` but only firmware's
     `tasks/menu/mod.rs` checks it
   - The app crate's `menu_task()` should handle it too so desktop can launch WASM stubs

### Medium Priority (Code quality)

1. **Move Power Status Polling into Manager**
   - Similar to LED's work loop pattern
   - Would remove `power_monitoring_task` from `i2c.rs`

2. **Add feature flags to `app` crate**
   - `#[cfg(feature = "std")]` for desktop-specific impls (e.g. sleep via std::thread)
   - `#[cfg(feature = "embassy")]` for embassy-based impls
   - Currently the crate is `no_std` with embassy deps, which is fine for both

3. **Remove `esp_hal::system::software_reset()` calls in `wifi_join.rs` and `*_updater.rs`**
   - Replace with `Platform::software_reset()` which is already in the trait
   - `wifi_join.rs` line 56 still calls `esp_hal::system::software_reset()` directly

4. **Clean up warnings across all crates**
   - Many unused imports from the refactoring
   - `app` crate has ~20 warnings, firmware has ~45

### Low Priority (Testing & Desktop)

1. **Desktop crate improvements**
   - Render icons (currently empty on non-xtensa)
   - Add WASM stubs so FilesApp "Execute" shows a message instead of hanging
   - Add network stack for AppStore/OTA testing
   - Improve keyboard mappings

2. **Add unit tests to `app` crate**
   - Create `app/tests/` directory
   - Use `MockPlatform` (from desktop or a dedicated test module) for deterministic tests

3. **Add `#[serde(default)]` to new `DeviceConfig` fields**
   - Any field added after initial release needs `#[serde(default)]` to avoid
     breaking deserialization of existing stored configs (e.g., `ap_password`)

## Key Decisions & Rationale

### Why Traits Instead of Concrete Types

- **Testability**: Mock implementations without hardware
- **Flexibility**: Swap implementations without changing app code
- **Separation of Concerns**: App doesn't know about embassy, I2C buses, etc.

### Why Cloneable Handles

- Managers wrapped in `Arc<dyn Trait>` for shared ownership
- Handles (`LedHandle`, `PowerHandle`) clone cheaply
- Allows passing to tasks/spawned code without lifetime issues
- **Prefer `Clone` over `Sync` for crossing cores** — cloning an `Arc` is cheap and doesn't
  require `unsafe` assertions. Avoid adding `Sync` bounds to traits unless the type genuinely
  needs shared mutable access across threads.

### Handle Design: `Deref` over Proxying

- Handle types (like `StorageHandle`) should expose the inner trait via `Deref` rather than
  proxying every method. This eliminates boilerplate while keeping the full API available.
- Example pattern:
  ```rust
  pub struct StorageHandle {
    inner: Arc<dyn LocalFsTrait>,
  }
  impl Deref for StorageHandle {
    type Target = dyn LocalFsTrait;
    fn deref(&self) -> &Self::Target { &*self.inner }
  }
  ```
- Callers then use `handle.method().await` directly — method resolution goes through `Deref`
  to the inner trait object. No proxy methods needed.
- The tradeoff: when the inner trait takes owned params (e.g. `String` instead of `&str`),
  callers must convert explicitly (`"foo".to_string()`). Accept this — it's better than
  maintaining proxy methods that accept `&str` and convert internally.

### Why Async Methods in Traits

- I/O operations should be async in Embassy
- `Pin<Box<dyn Future>>` allows trait object safety
- Alternative: Builder pattern with `.send().await` would leak channels

### RwLock Preference Over Mutex

**Pattern:** Use `RwLock<CriticalSectionRawMutex, T>` instead of `Mutex<CriticalSectionRawMutex, T>` for all internal synchronization.

**Why RwLock:**
- Better for read-heavy operations (e.g., `get_status()` may be called frequently by many code paths)
- `CriticalSectionRawMutex` is always `Sync`, no unsafe impls needed
- Allows concurrent readers while exclusive writers still work correctly
- Consistent with existing patterns in the codebase (used in menu state)

**Example:** HardwarePowerManager uses `Arc<RwLock<CriticalSectionRawMutex, Bq25895<I2C>>>` to wrap the charger IC.

**Lesson:** Prefer `RwLock` with `CriticalSectionRawMutex` for managers. Avoid `NoopRawMutex` in trait implementations since it's not `Sync` and would require unsafe escapes.

## Testing Strategy

### Current State
- Manual testing on device only
- No way to test menu logic without flashing

### Target State

1. **Unit Tests** - Test individual app logic
   - Create `app/tests/` directory
   - Use `MockPlatform` for deterministic tests
   - No hardware or async runtime needed

2. **Integration Tests** - Test app with mocked device
   - Spawn menu with `MockPlatform`
   - Simulate button presses and check LED requests
   - Verify WiFi connect logic without real network

3. **Local Development** - Run on localhost
   - `cargo run --example menu_standalone` with mock platform
   - Useful for UI iteration without device

## Common Pitfalls & Solutions

### Pitfall: Leaking Types to App Code
**Solution:** Never expose `Sender<T>`, `Receiver<T>`, `Watch<T>`, or other synchronization types in Platform trait. Encapsulate them in manager implementations. Keep only domain types (e.g., `WifiStatus`, `LedRequest`) in public APIs.

**Example of Wrong Pattern:**
```rust
// BAD: Leaks watch type
pub trait WiFiManager {
  fn status_watch(&self) -> WifiStatusWatchReceiver;
}
```

**Example of Right Pattern:**
```rust
// GOOD: Async method hides implementation
pub trait WiFiManager {
  fn wait_for_status_change(&self) -> Pin<Box<dyn Future<Output = WifiStatus>>>;
}
```

### Pitfall: Trait Methods That Can't Be Mocked
**Example:** Methods that return concrete types that require hardware.
**Solution:** Use associated types or generic returns, ensure mock can construct dummy values.

### Pitfall: Lifetime Issues with Platform Handles
**Problem:** `Platform` trait can't be `'static` if managers hold references.
**Solution:** Wrap everything in `Arc` and use trait objects. Handles become `Arc<dyn Trait>`.

### Pitfall: Sync Issues with Non-Sync I2C Types
**Problem:** `NoopRawMutex` breaks trait object storage in Arc.
**Solution:** Add unsafe impl carefully, document safety rationale, consider fixing root cause (upgrade to CriticalSectionRawMutex) if possible.

### Pitfall: Littlefs `Filesystem` is not `Send`
**Problem:** `littlefs_rust::Filesystem` wraps C code with raw pointers (`*mut c_void`, `*mut u8`),
making it `!Send`. Storing it in `Arc<Mutex<CriticalSectionRawMutex, Filesystem>>` requires
`Filesystem: Send`, otherwise the compiler rejects it.

**Solution:** Wrap in a `SendFilesystem<S>` newtype with `unsafe impl<S: Storage> Send` and
`Deref`/`DerefMut` to the inner `Filesystem`. Sound because all access is serialized through
the `Mutex` and littlefs is reentrant for individual `lfs_t` handles.

```rust
struct SendFilesystem<S: Storage>(Filesystem<S>);
unsafe impl<S: Storage> Send for SendFilesystem<S> {}
impl<S: Storage> Deref for SendFilesystem<S> { type Target = Filesystem<S>; ... }
impl<S: Storage> DerefMut for SendFilesystem<S> { ... }
```

### Pitfall: `Pin<Box<dyn Future>>` methods taking `&str` params
**Problem:** An object-safe trait that returns `Pin<Box<dyn Future + Send + '_>>` cannot borrow
function parameters in the returned future — the future must outlive the call, but params are
dropped when the function returns.

**Solution:** Take owned types (`String`, `Vec<u8>`) instead of references (`&str`, `&[u8]`).
Move them into `async move { }` blocks inside the `Box::pin(...)`. Callers convert with
`.to_string()`, `.to_vec()`, or `.clone()`.

### Pitfall: Duplicate State Representations
**Problem:** Having both `WifiStatus` and `WifiStatusMessage` with conversion logic between them.
**Solution:** Use a single enum for state throughout the codebase. The watch stores the same type as the application sees. Keep state centralized - one enum, one source of truth.

### Pitfall: `Rgb565::new()` treats args as raw 5/6/5 values
**Problem:** `embedded_graphics::pixelcolor::Rgb565::new(r, g, b)` expects `r` in 0–31, `g` in 0–63,
`b` in 0–31 (the raw channel bit depths). Passing 8‑bit values (0–255) produces wrong colours.
**Solution:** Always construct from 8‑bit values via `Rgb565::from(Rgb888::new(r, g, b))` — this does
the proper 8‑bit→5/6/5 conversion with correct rounding. Or use the constants
(`Rgb565::WHITE`, `Rgb565::BLACK`, etc.) when possible.

## Related Files

- **Main trait definitions:** `firmware/src/platform/traits.rs`
- **Platform implementations:** `firmware/src/platform/{display,led,power,wifi,etc}.rs`
- **Storage manager:** `firmware/src/platform/storage.rs`
- **WiFi status monitoring:** `firmware/src/tasks/wifi_monitor.rs`
- **WatchedValue primitive:** `firmware/src/utils/watched_value.rs`
- **EventQueue primitive:** `firmware/src/utils/event_queue.rs`
- **Application entry point:** `firmware/src/bin/rustagon.rs`
- **Menu system (to be extracted):** `firmware/src/tasks/menu/`
- **Shared types:** `firmware/src/types.rs`
- **Network infrastructure:** `firmware/src/tasks/net.rs`
- **ESP32-S3 flash driver:** `esp32s3_embedded_tools/src/flash.rs`
- **Generic LocalFs (littlefs):** `embedded_tools/src/local_fs.rs`
