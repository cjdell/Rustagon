# Architecture & Development Notes for Agents

## Current Architecture

The firmware is structured around a **Platform abstraction** that provides trait-based access to hardware subsystems. This allows the application code to be independent of specific hardware implementations and enables testing with mock implementations.

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

## Target Architecture (End State)

The goal is to split the firmware into **three crates** with the platform abstraction as the boundary:

```
rustagon/
├── app/              # Platform-agnostic application library
│   ├── menu/         # Menu system (currently in tasks/menu)
│   ├── apps/         # App implementations
│   ├── protocol/     # Application protocol definitions
│   └── lib.rs
├── firmware/         # Hardware-specific implementation
│   ├── src/platform/ # Platform implementations (HW + mocks)
│   ├── src/bin/rustagon.rs # Only initialization and task spawning
│   └── lib.rs
└── Cargo.toml        # Workspace
```

### Application Library Requirements

The `app` crate should:
- Depend ONLY on the `Platform` trait, never concrete types
- Never import from `firmware/src/tasks/*` directly
- Not know about channels, I2C buses, or hardware details
- Be compilable on localhost for unit/integration testing

**Currently blocking this:**
- `MenuRunnerContext` still contains Display and Input hardware-specific types
- Application tasks mixed with framework tasks
- WiFi, Power, and LED managers have been abstracted

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

6. **Creating Helper Primitives**
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

### High Priority (Unblock app library extraction)

1. **Display/LCD Manager** (`firmware/src/platform/display/`)
   - Currently `lcd_signal: &'static LcdSignal` in MenuRunnerContext
   - Need: `DisplayManager` trait with methods like `show_screen(Screen)`, `clear()`
   - Challenge: Lifetimes of static LCD signal - may need different approach

2. **Input/Button Manager** (`firmware/src/platform/input/`)
   - Currently `hex_button_subscriber: HexButtonReceiver` in MenuRunnerContext
   - Need: `InputManager` trait with event subscription
   - Pattern: Observer/subscription pattern via trait

3. ~~**Network/WiFi Manager**~~ ✅ **COMPLETED**
     - Implemented in `firmware/src/platform/wifi/`
     - High-level API with connect, scan, get status
     - Async state machine handled by manager background task
     - Uses `WatchedValue<WifiStatus>` for clean state management
     - Monitor task accesses via Platform trait - no extra receivers needed

4. **Storage Manager** (`firmware/src/platform/storage/`)
   - Currently: Direct access to `device_state: DeviceState` and `local_fs: LocalFs`
   - Need: Trait-based access to config/file operations
   - Challenge: Return types for file handles and iterators

5. **Refactor MenuRunnerContext**
   - Make it generic over `<P: Platform>`
   - Remove all channel types - only use Platform
   - This enables testing with MockPlatform

### Medium Priority (Code quality)

1. **Move Power Status Polling into Manager**
   - Similar to LED's work loop pattern
   - Would remove `power_monitoring_task` from `i2c.rs`

2. **Create PlatformAgnostic Type Aliases**
   - Define commonly-needed types in a way that doesn't depend on embassy_sync
   - Example: `type NetworkStatus` instead of exposing WifiStatusMessage

3. **Clean Up Imports in Menu Module**
   - Remove direct imports of firmware-specific types
   - Use only types from `Platform` trait

### Low Priority (Optimization)

1. **Add Feature Flags**
   - `#[cfg(feature = "mock-platform")]` for test builds
   - Allow `cargo build --example menu_standalone --features mock-platform`

2. **Implement Default MockPlatform**
   - Provide a fully-functional mock for running app logic locally
   - Useful for developing UI without hardware

## Key Decisions & Rationale

### Why Traits Instead of Concrete Types

- **Testability**: Mock implementations without hardware
- **Flexibility**: Swap implementations without changing app code
- **Separation of Concerns**: App doesn't know about embassy, I2C buses, etc.

### Why Cloneable Handles

- Managers wrapped in `Arc<dyn Trait>` for shared ownership
- Handles (`LedHandle`, `PowerHandle`) clone cheaply
- Allows passing to tasks/spawned code without lifetime issues

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

### Pitfall: Duplicate State Representations
**Problem:** Having both `WifiStatus` and `WifiStatusMessage` with conversion logic between them.
**Solution:** Use a single enum for state throughout the codebase. The watch stores the same type as the application sees. Keep state centralized - one enum, one source of truth.

## Related Files

- **Main trait definitions:** `firmware/src/platform/traits.rs`
- **Platform implementations:** `firmware/src/platform/{led,power,wifi,etc.}/`
- **WiFi status monitoring:** `firmware/src/tasks/wifi_monitor.rs`
- **WatchedValue primitive:** `firmware/src/utils/watched_value.rs`
- **Application entry point:** `firmware/src/bin/rustagon.rs`
- **Menu system (to be extracted):** `firmware/src/tasks/menu/`
- **Shared types:** `firmware/src/types.rs`
- **Network infrastructure:** `firmware/src/tasks/net.rs`
