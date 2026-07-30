# Hexpansion System

The Tildagon badge at EMF 2024 supports expansion boards called **hexpansions**
that plug into six physical edge ports around the badge's hexagonal PCB. Port 0
is the "frontboard" (the hexagonal touch panel face) which uses the same protocol.

This document describes how the hexpansion system works in the original C/MicroPython
firmware and proposes how to bring it into the Rustagon project.

---

## Hardware Architecture

### I2C Multiplexer (TCA9548A)

The badge has a single hardware I2C bus (`I2C_NUM_0`, SDA=GPIO45, SCL=GPIO46,
133 kHz) which is split into 8 virtual buses by a **TCA9548A** I2C multiplexer
at address `0x77`. Only one downstream port can be active at a time; the mux
selects the port before each transaction.

| Port Constant            | ID | Purpose                     |
|--------------------------|----|-----------------------------|
| `TILDAGON_TOP_I2C_PORT`  | 0  | Frontboard (touch, IMU, top USB-C power) |
| `TILDAGON_HX0_I2C_PORT`  | 1  | Hexpansion port 1           |
| `TILDAGON_HX1_I2C_PORT`  | 2  | Hexpansion port 2           |
| `TILDAGON_HX2_I2C_PORT`  | 3  | Hexpansion port 3           |
| `TILDAGON_HX3_I2C_PORT`  | 4  | Hexpansion port 4           |
| `TILDAGON_HX4_I2C_PORT`  | 5  | Hexpansion port 5           |
| `TILDAGON_HX5_I2C_PORT`  | 6  | Hexpansion port 6           |
| `TILDAGON_SYS_I2C_PORT`  | 7  | Internal badge systems       |

Each hexpansion port connects to a dedicated channel on the TCA9548A mux, so
devices on different ports can share I2C addresses without conflict.

### On-Badge I2C Devices (System Bus, Port 7)

The system bus (port 7) hosts the badge's internal peripherals:

| Device        | I2C Address | Purpose                            |
|---------------|-------------|------------------------------------|
| BQ25895       | `0x6A`      | Battery charger / power management |
| FUSB302B (in) | `0x22`      | USB-C PD controller (device port)  |
| AW9523B #0    | `0x58`      | GPIO expander (bank 0, pins 0-15)  |
| AW9523B #1    | `0x59`      | GPIO expander (bank 1, pins 16-31) |
| AW9523B #2    | `0x5A`      | GPIO expander (bank 2, pins 32-47) |

### Top Bus I2C Devices (Port 0)

Port 0 is the "top" port used by the frontboard — the hexagonal touch panel
faceplate. Devices on this bus:

| Device          | I2C Address | Purpose                              |
|-----------------|-------------|--------------------------------------|
| CY8CMBR3116     | `0x37`      | Capacitive touch controller          |
| FUSB302B (out)  | `0x22`      | USB-C PD controller (host port)      |
| AW9523B #3      | `0x58`      | GPIO expander (bank 3, frontboard)   |
| Frontboard EEPROM | `0x50`-`0x57` | Identity header and filesystem    |

The two FUSB302B devices share address `0x22` but are on different mux ports
(in on port 7 via system mux, out on port 0 via top mux).

The 4th AW9523B at `0x58` on the top bus was added with the **2026 frontboard**
("Spaceagon") revision. It provides the hex buttons and additional frontboard
GPIOs. It shares address `0x58` with AW9523B #0 on the system bus but they are
on different mux channels, so there is no conflict.

The frontboard EEPROM stores a `HexpansionHeader` exactly like any other
hexpansion, using VID `0xBAD3` and PID `0x2400` (2024) or `0x2600` (2026).



### AW9523B GPIO Expanders

Four AW9523B chips provide GPIO expansion — three on the system bus and one on
the top bus (added with the 2026 frontboard rev):

| Bank | I2C Addr | Mux Port | GPIO Ext Numbers | Pins                         |
|------|----------|----------|------------------|------------------------------|
| 0    | `0x58`   | 7 (SYS)  | 0-15             | Frontboard buttons           |
| 1    | `0x59`   | 7 (SYS)  | 16-31            | Frontboard buttons           |
| 2    | `0x5A`   | 7 (SYS)  | 32-47            | LED power, USB mux, 5V switch|
| 3    | `0x58`   | 0 (TOP)  | 48-63            | Hex buttons (2026 frontboard)|

Each AW9523B supports 16 pins, configurable as GPIO or LED (with current drive
control), and supports edge-triggered IRQs on any pin.

In the Rust firmware (`firmware/src/d_i2c.rs`), the four chips are initialized
at boot as `I2C_0` through `I2C_3`:

```rust
pub const I2C_0: u8 = 0x58; // SYS_BUS — bank 0
pub const I2C_1: u8 = 0x59; // SYS_BUS — bank 1
pub const I2C_2: u8 = 0x5A; // SYS_BUS — bank 2
pub const I2C_3: u8 = 0x58; // TOP_BUS — bank 3 (2026 frontboard)
```

Initialization in `rustagon.rs` currently covers the three system-bus chips:

```rust
init_gpio(sys_bus.clone(), I2C_0).await;
init_gpio(sys_bus.clone(), I2C_1).await;
init_gpio(sys_bus.clone(), I2C_2).await;
```

(Initialization of `I2C_3` on the top bus is not yet called in the production
entry point — only in the standalone `i2c.rs` test binary.)

---

## Hexpansion Port Hardware (Physical Layer)

### Connector Pins

Each hexpansion port exposes:

- **I2C bus** (SDA/SCL via the TCA9548A mux channel)
- **4 high-speed GPIOs** (`hs`) — direct ESP32-S3 GPIOs for fast digital I/O
- **5 low-speed GPIOs** (`ls`) — via the AW9523B GPIO expanders for detect/level
- **3.3V power** (up to 500mA per port)
- **5V power** (switched, for higher-power expansions)

### Frontboard Button Pin Mappings (2024 frontboard, AW9523B #0 / #1)

From the Rust firmware (`firmware/src/platform/input.rs`):

| Button | AW9523B Bank | Pin | Function       |
|--------|--------------|-----|----------------|
| A/Up   | #2 (`0x5A`)  | P06 | Navigate up    |
| B/Right| #2 (`0x5A`)  | P07 | Navigate right |
| C/Fire | #1 (`0x59`)  | P00 | Confirm/fire   |
| D/Down | #1 (`0x59`)  | P01 | Navigate down  |
| F/Left | #1 (`0x59`)  | P03 | Navigate left  |

### Hex Button Pin Mappings (2026 frontboard, AW9523B #3 on TOP_BUS)

The six edge hex buttons on the 2026 frontboard are read from AW9523B #3
(address `0x58` on the top bus, mux port 0) in `firmware/src/platform/input.rs`:

| Button | AW9523B #3 Pin | Function    |
|--------|----------------|-------------|
| HexA   | P12            | Hex button 1|
| HexB   | P11            | Hex button 2|
| HexC   | P10            | Hex button 3|
| HexD   | P15            | Hex button 4|
| HexE   | P14            | Hex button 5|
| HexF   | P13            | Hex button 6|

These are polled in the `button_monitoring_task` alongside the frontboard
buttons, using the same edge-detect state machine.

### Hexpansion Port Pin Mappings

Each hexpansion port exposes 4 HS GPIOs (direct ESP32-S3 pins) and 5 LS GPIOs
(via AW9523B expanders). The LS pins also serve as presence detect.

| Port | HS GPIO Pins          | LS ePin Names                          |
|------|-----------------------|----------------------------------------|
| 1    | 39, 40, 41, 42        | `1_LS_A`..`1_LS_E`                     |
| 2    | 35, 36, 37, 38        | `2_LS_A`..`2_LS_E`                     |
| 3    | 34, 33, 47, 48        | `3_LS_A`..`3_LS_E`                     |
| 4    | 11, 14, 13, 12        | `4_LS_A`..`4_LS_E`                     |
| 5    | 18, 16, 15, 17        | `5_LS_A`..`5_LS_E`                     |
| 6    | 3, 4, 5, 6            | `6_LS_A`..`6_LS_E`                     |

In the original C firmware, the `ePin` LS names map to AW9523B pins through
the `tildagonos.py` EPIN_ND constants:

```python
EPIN_ND_A = (2, 12)  # bank 2, pin 12 (AW9523B #2)
EPIN_ND_B = (2, 13)
EPIN_ND_C = (1, 8)   # bank 1, pin 8  (AW9523B #1)
EPIN_ND_D = (1, 9)
EPIN_ND_E = (1, 10)
EPIN_ND_F = (1, 11)
```

The `HexpansionConfig` class in MicroPython exposes both sets to hexpansion
apps as `pin` (4 HS `Pin` objects) and `ls_pin` (5 LS `ePin` objects).

### Presence Detection via LS Pins

When a hexpansion is inserted, the LS detect pin (pin `A` on each port) changes
state. The `HexpansionManagerApp` registers IRQ handlers on the six LS detect
pins. On falling edge → `HexpansionInsertionEvent(port)` is emitted. On rising
edge → `HexpansionRemovalEvent(port)` is emitted. At init time, any pin
already low triggers insertion events for those ports.

---

## Hexpansion Identity Protocol

### EEPROM Detection

On insertion, the firmware scans the hexpansion's I2C bus for EEPROM devices
in the `0x50`-`0x57` range. The detection logic in `detect_eeprom_addr()`:

1. If only `0x57` is present → 2-byte addressing, address `0x57`
2. If all eight `0x50`-`0x57` are present → 1-byte addressing, address `0x50`
3. If only `0x50` is present → 2-byte addressing, address `0x50`
4. Otherwise → no EEPROM detected

### Header Format: `HexpansionHeader`

The header is a **32-byte** record stored at EEPROM address `0x00`:

```
Offset  Size  Field             Type         Description
------  ----  ----------------- ------------ ----------------------------
 0      4     magic             bytes        "THEX" (0x54 48 45 58)
 4      4     manifest_version  bytes        "2024" or "2026"
 8      2     fs_offset         uint16 LE    Offset of LittleFS partition
10      2     eeprom_page_size  uint16 LE    EEPROM page size in bytes
12      2     eeprom_total_size uint16 LE    Total EEPROM size in bytes
14      2     vid               uint16 LE    Vendor ID
16      2     pid               uint16 LE    Product ID
18      4     unique_id         uint32 LE    Unique serial number
22      9     friendly_name     char[9]      Null-terminated name string
31      1     checksum          uint8        XOR of bytes 1..30 (seed 0x55)
```

The checksum is computed as `0x55 ^ byte[1] ^ byte[2] ^ ... ^ byte[30]`.

The `magic` field is `"THEX"`. `manifest_version` is `"2024"` or `"2026"`.
`fs_offset` indicates where the LittleFS filesystem starts within the EEPROM
(the header is at offset 0 and is not part of the filesystem). `eeprom_page_size`
determines the write chunk size (typically 32 or 64 bytes).

Because MicroPython's `struct` module lacks `__eq__` for binary roundtrips,
`friendly_name` is stored as a fixed 9-byte field (padded with nulls) and
decoded by splitting on `\x00`.

### Reading the Header

```python
header_bytes = i2c.readfrom_mem(eeprom_addr, 0, 32, addrsize=addr_len * 8)
header = HexpansionHeader.from_bytes(header_bytes)
```

The `addrsize` parameter is either 8 (1-byte address) or 16 (2-byte address).

### Writing the Header

Writing is done in page-sized chunks, polling for ACK after each write:

```python
for idx, chunk in enumerate(header_chunks):
    write_addr = struct.pack(addr_pack, idx * page_size)
    i2c.writeto(addr, write_addr + chunk)
    while True:
        try:
            if i2c.writeto(addr, bytes([0])):
                break
        except OSError:
            pass
        time.sleep_ms(1)
```

---

## Hexpansion Lifecycle

### Insertion Flow

1. **GPIO IRQ fires** — AW9523B detects LS pin state change
2. **`HexpansionInsertionEvent(port)`** emitted on eventbus
3. **`handle_hexpansion_insertion`** handler runs (async):
   a. Scan I2C bus on the port's mux channel for EEPROM addresses
   b. Retry once after 100ms if no EEPROM found (debounce)
   c. Acquire/release `handle_insertion_lock` (for provisioning coordination)
   d. Read 32-byte `HexpansionHeader` from EEPROM
   e. Create block devices: `EEPROM` + `EEPROMPartition` for the filesystem area
   f. Attempt to mount the partition as LittleFS at `/hexpansion_{port}`
   g. Scan for a manifest at the mount point
   h. Emit `HexpansionMountedEvent`
   i. If `autolaunch=True`, attempt to load and start the hexpansion's app

### Removal Flow

1. **GPIO IRQ fires** — AW9523B detects LS pin rising edge
2. **`HexpansionRemovalEvent(port)`** emitted on eventbus
3. **`handle_hexpansion_removal`** handler runs (async):
   a. Stop the hexpansion app if running
   b. Remove app launcher entry if present
   c. Clear cached header, manifests
   d. Unmount the filesystem
   e. Reset all LS/HS pins to default (input) state
   f. Emit `HexpansionUnmountedEvent`

### Filesystem Layout

The EEPROM is divided into two regions:

- **Header**: bytes `0x00`–`0x1F` (32 bytes) — the `HexpansionHeader`
- **Filesystem**: starts at `fs_offset` (typically 64) — a LittleFS v2
  partition mounted via `vfs.VfsLfs2`

The `EEPROMPartition` class wraps the EEPROM with an offset/length, presenting
a block device to `vfs.mount()`. Block size is 9 (512 bytes) for EEPROMs >=
8KiB, or 6 (64 bytes) for smaller chips.

### App Loading

When a hexpansion is mounted, the system looks for an app at:
```
/hexpansion_{port}/app.py   (exports __app_export__)
```

If not found, it falls back to a driver in flash:
```
/drivers/hex_{vid:04x}_{pid:04x}/app.py
```

Apps receive a `HexpansionConfig(port)` object with:
- `port`: the mux port number
- `i2c`: an `I2C(port)` object for the hexpansion's bus
- `pin`: list of 4 `Pin` objects (HS GPIOs)
- `ls_pin`: list of 5 `ePin` objects (LS GPIOs via AW9523B)

### Firmware Updates

The `hexpansionfw.py` app can download firmware from a GitHub release repo:
```
https://github.com/emfcamp/hexpansion-firmwares/releases/download/latest/
firmware_0x{vid:04x}_0x{pid:04x}.tar.gz
```

The tarball is extracted onto the hexpansion's LittleFS, overwriting existing
files. If the hexpansion is not mounted (no filesystem), the files are written
to `/drivers/hex_{vid:04x}_{pid:04x}/` in internal flash instead.

---

## Event System

All hexpansion lifecycle events flow through the eventbus:

| Event                              | Payload      | When                               |
|------------------------------------|--------------|------------------------------------|
| `HexpansionInsertionEvent`         | `port`       | LS pin falling edge (insert)       |
| `HexpansionRemovalEvent`           | `port`       | LS pin rising edge (remove)        |
| `HexpansionMountedEvent`           | `port,header`| Filesystem mounted                 |
| `HexpansionUnmountedEvent`         | `port,header`| Filesystem unmounted               |
| `HexpansionFormattedEvent`         | `port`       | EEPROM formatted                   |
| `HexpansionAppRequestStartEvent`   | `port`       | External request to start app      |
| `HexpansionAppRequestStopEvent`    | `port`       | External request to stop app       |
| `HexpansionAppLauncherAddEvent`    | `port,name`  | App added to launcher              |
| `HexpansionAppLauncherRemoveEvent` | `port`       | App removed from launcher          |

---

## Provisioning

The `HexpansionInfoApp` (hexpansionfw) allows:

- **Manual VID/PID entry** — for unprovisioned EEPROMs
- **Search** — fetches header metadata from GitHub based on VID/PID
- **Firmware update** — downloads + extracts firmware tarball
- **Factory reset** — reformats EEPROM and rewrites header + firmware
- **Bulk provisioning** — listens for insertion events and provisions each
  hexpansion with incremented unique IDs

The `mount_hexpansions.py` script provides a simple boot-time mount for all
six ports without the full manager app.

---

## Proposal for Rustagon Integration

### Core Design Principles

1. **Platform abstraction** — Hexpansion support should live in `app/` as a
   platform-agnostic subsystem, with hardware-specific I2C/GPIO glue in
   `firmware/` and `desktop/`
2. **Event-driven** — Use `EventQueue<T,N>` for insertion/removal events,
   not polling
3. **MenuApp integration** — Hexpansion detection should be a system service
   that feeds into the app stack, not a standalone app
4. **Desktop simulation** — Hexpansions should be simulatable on desktop
   (e.g., config files mapping virtual ports to mock EEPROM images)

### Architecture

```
app/
  hexpansion/
    mod.rs           # HexpansionManager<P: Platform> — the system service
    header.rs        # HexpansionHeader struct + serialize/deserialize
    events.rs        # HexpansionEvent, InsertionEvent, RemovalEvent, ...
    config.rs        # HexpansionConfig (port, i2c handle, pins)
    types.rs         # VID/PID, PortId, types
    loader.rs        # App loading from filesystem (WASM or native)

firmware/
  src/
    platform/
      hexpansion.rs  # Hardware detection: I2C scan, AW9523B IRQ handling
    tasks/
      hexpansion.rs  # Embassy task: monitor LS pins, emit events
```

### Key Components

#### 1. `HexpansionHeader` in `app/src/hexpansion/header.rs`

```rust
#[repr(C, packed)]
struct HexpansionHeader {
    magic: [u8; 4],           // "THEX"
    manifest_version: [u8; 4], // "2024"
    fs_offset: u16_le,
    eeprom_page_size: u16_le,
    eeprom_total_size: u16_le,
    vid: u16_le,
    pid: u16_le,
    unique_id: u32_le,
    friendly_name: [u8; 9],
    checksum: u8,
}
```

Implement `from_bytes()` with validation (magic, checksum). Add
`read_from_i2c(bus, addr, addr_len)` and `write_to_i2c(...)` methods.

#### 2. Port detection via the Platform trait

Add to the `Platform` trait:

```rust
async fn detect_hexpansions(&self) -> Vec<HexpansionEvent>;
fn hexpansion_i2c_bus(&self, port: u8) -> I2cHandle;
fn hexpansion_gpios(&self, port: u8) -> HexpansionGpioSet;
```

On firmware: scan each mux port (1-6) for EEPROMs, read headers.
On desktop: read from a config file or mock EEPROM images.

#### 3. Hot-plug support via AW9523B IRQ → EventQueue

In `firmware/src/platform/hexpansion.rs`:

- Register IRQ handlers on the 6 detect pins via AW9523B
- On falling edge → push `HexpansionInsertionEvent(port)` to an `EventQueue`
- On rising edge → push `HexpansionRemovalEvent(port)` to an `EventQueue`
- An Embassy task in `firmware/src/tasks/hexpansion.rs` reads from the queue
  and drives the detection/mount/load flow

#### 4. EEPROM I/O

A `HexpansionEeprom` abstraction wrapping the I2C handle:

```rust
struct HexpansionEeprom {
    i2c: I2cHandle,
    addr: u8,
    addr_len: u8,   // 1 or 2
    page_size: u16,
    total_size: u32,
}
```

Implement `read(pos, buf)` and `write(pos, data)` with page-boundary handling
and ACK polling.

#### 5. Filesystem via littlefs

The Rustagon project already has `littlefs_rust` available (used in the
firmware crate). Mount a LittleFS partition on the EEPROM block device,
starting at `fs_offset`.

#### 6. WASM app loading

When a hexpansion is mounted and its filesystem has a `app.wasm` (or similar),
the loader should:
1. Read the WASM binary from the EEPROM filesystem
2. Launch it via the existing WASM runtime
3. Pass it a `HexpansionConfig` with I2C handle and GPIO handles

Native apps (compiled into the firmware) can also register for specific
VID/PID pairs.

#### 7. Desktop simulation

On desktop, simulate hexpansions via:

- A JSON config file: `~/.rustagon/hexpansions/port_1.json` containing header
  fields + path to a mock EEPROM image file
- The I2C handle proxies reads/writes to the image file
- LS pin IRQ is triggered by file watching or a UI toggle

#### 8. Event flow in the menu system

The `menu_task` should integrate hexpansion events via `select3`:

```rust
select3(
    system_button,          // boot button press
    hex_button,             // hex button press
    hexpansion_event.receive(),  // insertion/removal
)
```

On insertion: mount EEPROM, read header, optionally launch app.
On removal: stop app, unmount, clean up.

### Migration Path

1. **Phase 1** — Implement `HexpansionHeader` parsing/writing in `app/src/hexpansion/`
   with full test coverage using `no_std` compatible code.

2. **Phase 2** — Add `EepromDevice` abstraction and `HexpansionManager<P>` with
   event-driven detection (no hot-plug initially — scan at boot).

3. **Phase 3** — Implement firmware hot-plug detection via AW9523B IRQ and the
   TCA9548A mux. Add LS pin handling to the firmware platform impl.

4. **Phase 4** — Add LittleFS mounting on EEPROM partitions. Implement
   `HexpansionConfig` and app loading.

5. **Phase 5** — Desktop simulation: mock EEPROM images, virtual ports, and
   I2C/GPIO simulation.

6. **Phase 6** — Firmware update/provisioning app and bulk provisioning.

### Files to Create

```
app/src/
  hexpansion/
    mod.rs          # HexpansionManager<P>, public API
    header.rs       # HexpansionHeader struct
    events.rs       # Event types for insertion/removal
    config.rs       # HexpansionConfig
    types.rs        # PortId, VidPid, EepromAddr
    loader.rs       # WASM/native app loading from hexpansion FS
```

### Files to Modify

```
app/src/
  platform/
    mod.rs          # Add hexpansion-related methods to Platform?
    traits.rs       # Or: add a HexpansionManager trait
    storage.rs      # Reuse StorageHandle for EEPROM?
  menu/
    mod.rs          # Add select3 for hexpansion events
    state.rs        # Add HostedApp variant for hexpansion WASM apps
    types.rs        # AppLoader integration

firmware/src/
  platform/
    mod.rs          # Add HardwareHexpansionManager
    system.rs       # Share AW9523B IRQ line with hexpansion detection
  tasks/
    mod.rs          # Add hexpansion monitoring task
    hexpansion.rs   # New: embassy task for hot-plug detection

desktop/src/
  platform/
    mod.rs          # Add DesktopHexpansionManager (file-based mock)
  tasks/wasm/       # Hexpansion app lifecycle
```

### Open Questions

1. **WASM app loading from EEPROM FS** — The current WASM runner loads programs
   from flash storage. Loading from an I2C EEPROM filesystem requires streaming
   the WASM binary into RAM first (EEPROMs are slow). Need to decide on a
   loading strategy.

2. **EEPROM write endurance** — I2C EEPROMs typically support ~1M write cycles.
   LittleFS wear-leveling helps, but firmware update writes the entire FS. The
   /drivers/ fallback path in flash avoids this for frequently-updated firmware.

3. **Power management** — Hexpansions draw power. The firmware should be able
   to disable 5V/3.3V to individual ports to save battery. The AW9523B already
   controls the 5V switch.

4. **Conflict resolution** — If two hexpansions on different ports have the
   same VID/PID but different apps, which one launches? The current system
   launches both independently.

5. **Desktop I2C simulation fidelity** — Minifb desktop cannot actually talk
   I2C. Building a file-backed EEPROM simulator that faithfully reproduces
   page-write behavior and ACK polling is important for app development.
