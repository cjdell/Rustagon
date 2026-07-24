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
- Internal `Arc<Mutex<>>` for shared access to I2C device
- Power control requests come directly through the manager (no external channel)

**Key Learning:** Async traits require `Pin<Box<dyn Future>>` for object safety. The `NoopRawMutex` in `MaskedI2cBus` required unsafe `Send + Sync` implementations to work with trait objects in `Arc`.

### Removed from Task Model

1. **LED channel** (`power_ctrl_channel`) - Removed from rustagon.rs
2. **Power control task** - Simplified to just status polling in `power_monitoring_task`
3. **Channel-based power control** - Replaced with direct `platform.power().power_off().await`

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
- `MenuRunnerContext` still contains hardware-specific types and channels
- Application tasks mixed with framework tasks
- Direct imports of `embassy_sync::channel` types throughout

### What Makes a Good Platform Manager

Based on LED and power implementations:

1. **Trait Design**
   - Minimal interface - only operations the app actually needs
   - Async methods when they do I/O or take significant time
   - Return types that can be stored (hence `Pin<Box<dyn Future>>` for async)

2. **Internal Encapsulation**
   - All channels/synchronization primitives internal to the manager
   - No channel types leak to the application
   - Work loops spawned by the manager if needed (like LED)

3. **Mock Implementations**
   - Must be implementable without any hardware
   - Should return reasonable default values for testing
   - Excellent for integration tests without an ESP32

4. **Hardware Implementation**
   - Uses concrete I2C/GPIO types from the crate
   - Protected by mutexes for safe concurrent access
   - May require `unsafe impl Send/Sync` if using NoopRawMutex

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

3. **Network/WiFi Manager** (`firmware/src/platform/network/`)
   - Currently channels: `wifi_command_sender`, `wifi_status_receiver`, `wifi_scan_watch`
   - Need: High-level API (connect, scan, get status)
   - Challenge: Async state machine for connections

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

### Pitfall: Leaking Channel Types to App Code
**Solution:** Never expose `Sender<T>`, `Receiver<T>`, or channel-related types in Platform trait. Encapsulate them in manager implementations.

### Pitfall: Trait Methods That Can't Be Mocked
**Example:** Methods that return concrete types that require hardware.
**Solution:** Use associated types or generic returns, ensure mock can construct dummy values.

### Pitfall: Lifetime Issues with Platform Handles
**Problem:** `Platform` trait can't be `'static` if managers hold references.
**Solution:** Wrap everything in `Arc` and use trait objects. Handles become `Arc<dyn Trait>`.

### Pitfall: Sync Issues with Non-Sync I2C Types
**Problem:** `NoopRawMutex` breaks trait object storage in Arc.
**Solution:** Add unsafe impl carefully, document safety rationale, consider fixing root cause (upgrade to CriticalSectionRawMutex) if possible.

## Related Files

- **Main trait definitions:** `firmware/src/platform/traits.rs`
- **Platform implementations:** `firmware/src/platform/{led,power,display,etc.}/`
- **Application entry point:** `firmware/src/bin/rustagon.rs`
- **Menu system (to be extracted):** `firmware/src/tasks/menu/`
- **Types that need abstraction:** `firmware/src/types.rs`
