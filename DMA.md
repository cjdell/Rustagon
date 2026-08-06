# DMA / Async SPI notes for the LCD (and what we learned the hard way)

This document records everything learned while trying to make the WASM → LCD frame
path faster / non-blocking: the measurements, the failed experiments, the esp-hal
SPI-DMA API specifics, and the gotchas. Read it before touching the display path
again.

**Bottom line up front:** the fastest implementation is the simplest one — a
single blocking SPI write straight from the guest's buffer, executed on core 1
inside the WASM host call. Every attempt to "improve" it (queue + flush on core 0,
async DMA with chunking) made it measurably worse. The SPI bus is the bottleneck,
and it cannot be beaten by any amount of cleverness on the CPU side.

---

## 1. The display pipeline and the math

- Panel: GC9A01, 240×240, RGB565 → **115,200 bytes per frame** (`FRAME_BYTES`).
- Bus: SPI2 at **80 MHz** (see `lcd_task` in `firmware/src/platform/display.rs`).
- Transfer time: `115200 × 8 bits / 80 MHz ≈ 11.5 ms` → **~87 fps hardware
  ceiling** for full frames. No software design can exceed this; the only way to
  get more "fps" is partial/tearing updates.
- `SpiExclusiveDevice` (`firmware/src/utils/spi.rs`) wraps a **blocking**
  `Spi<'_, Blocking>` — every `send_data` busy-waits the CPU for the whole
  transfer. This is fine on core 1 (nothing else runs there); it is **not** fine
  on core 0.

## 2. The three implementations and their measured results

### v0 — original (baseline): direct blocking write on core 1

`HardwareWasmHost::set_lcd_buffer` transmuted `SPI_DISPLAY_INTERFACE` into a
`&mut DisplayInterface` and did the command + pixel write synchronously from
core 1, then copied the frame into `BUFFER` for the WebSocket snapshot every
250 ms.

- **Frame rate:** best of the three (~80 fps, bus-bound).
- **Cost:** core 1 blocks ~11.5 ms per frame (irrelevant — the guest is the only
  task on that core).

### v1 — ping-pong queue + blocking flush on core 0 (WORSE + stalls)

`RawFrameQueue` (two PSRAM slots, atomic state machine, `Signal` wake): WASM host
on core 1 copies the guest frame into a slot (fast memcpy) and signals; `lcd_task`
on core 0 flushes it to SPI.

- **Frame rate:** worse than v0.
- **New symptom:** lockup for ~half a second every few seconds.
- **Why:** the flush busy-waited core 0 for 11.5 ms per frame. Core 0 also runs
  the menu, wifi, net, HTTP, WebSocket, and the hexpansion I2C scan (every 2 s).
  Every WASM frame became coupled to core 0's availability: periodic core-0 work
  delayed flushes, and the flushes starved everything else. Before this change,
  WASM never depended on core 0 at all.

### v2 — single slot + async DMA on core 0 (WORST: ~5 fps)

`SpiDmaBus<'_, Async>` + `DMA_CH0` + a 16 KiB DMA `tx_buf`. `flush_frame` sent the
window commands and the pixels via `write_async`, which chunks the 115 KB frame
into the DMA buffer and yields between chunks.

- **Frame rate:** ~5 fps (~200 ms/frame).
- **Why:** **15 interrupt-driven waits per frame** — 7 tiny command writes + 8
  pixel chunks (16 KiB each). Every `write_async` pays: `wait_for_idle_async`
  (interrupt → executor wake → poll round trip) + `setup_full_duplex` +
  `start_dma_transfer` overhead. Small transfers amplify the fixed per-transfer
  latency. It also cannot beat the 87 fps bus ceiling anyway.

### v3 — final (fastest): v0's write, behind the platform abstraction

`DisplayManager::signal_raw_frame(&[u8])` was added to the trait
(`app/src/platform/display.rs`). The firmware impl of it does **exactly what v0
did** (direct blocking write from the calling core + 250 ms-throttled `BUFFER`
snapshot), but the code now lives in `HardwareDisplayManager` instead of the WASM
host, and `HardwareWasmHost::set_lcd_buffer` is just
`display.signal_raw_frame(buffer)`.

- **Frame rate:** noticeably faster than v0 (the user's measurement). Same bus
  ceiling, zero copies, zero indirection.
- **Why faster than v0 (probable):** the frame path is byte-identical, so the
  gain is most likely from build/optimization differences and less per-call
  churn — not from a design change. Treat it as "at least as fast as v0".

## 3. Lessons learned (apply before redesigning this path)

1. **The SPI bus is the bottleneck. Optimize the transfer, not the handoff.**
   Any design that performs one contiguous transfer per frame hits the same
   ~87 fps ceiling; extra copies, queues, and wake-ups only add cost.
2. **Never busy-wait a blocking SPI flush on core 0.** Core 0 runs the entire
   system (menu, wifi, net, HTTP, hexpansion polling). A blocking flush on core 0
   couples the display to every other subsystem and causes periodic stalls.
3. **Async DMA ≠ fast for small transfers.** `write_async` on `SpiDmaBus` pays a
   full interrupt round trip per chunk (max 32,736 bytes per DMA transfer). Many
   small writes (e.g. per-command) are disproportionately expensive — this is
   what produced 5 fps.
4. **If async DMA is ever revisited**, the design must be: **one DMA transfer per
   frame** (or as few as possible), commands batched or sent on the blocking path,
   and a large `tx_buf`. Chunking a 115 KB frame into 8 interrupt waits per frame
   is a non-starter. A single descriptor chain (one `start_dma_transfer` for the
   whole frame) would be the right shape.
5. **Keep the flush on the core that renders the frame.** Core 1 blocks harmlessly
   for 11.5 ms; core 0 does not.
6. **Zero-copy from guest memory** (writing straight from the WASM canvas via
   DMA) was never the win it sounds like: the guest memory is PSRAM, needs cache
   flush before DMA reads it, and the guest overwrites it mid-transfer. The
   blocking path reads it directly, so tearing is the only artifact.

## 4. esp-hal SPI/DMA API notes

Verified against esp-hal commit `1912938ac51e8db13a21e0aaafb5aafff9a6d3aa`
(`unstable` feature enabled in `firmware/Cargo.toml`). All line numbers below
refer to `esp-hal/src/spi/master.rs`.

- **Turning SPI into DMA:** `Spi::new(...).with_sck(..).with_mosi(..).with_dma(ch)`
  → `SpiDma<'d, Blocking>`. `ch` must be `impl DmaChannelFor<AnySpi<'d>>` — the
  S3 GDMA channels are fully flexible, so `DMA_CH0` worked. The wifi driver does
  **not** consume the user-visible GDMA channels, so `DMA_CH0` was free.
- **`SpiDmaBus` needs both RX and TX buffers** even for a write-only LCD:
  `SpiDmaBus::new(spi_dma, rx_buf, tx_buf)` (line ~2352). Allocate with the
  `dma_buffers!(rx_size, tx_size)` macro.
- **`dma_buffers!` is exported at the crate root** — `use esp_hal::dma_buffers;`,
  *not* `esp_hal::dma::dma_buffers` (that import fails with `unresolved import`).
  One-arg `dma_buffers!(n)` makes RX and TX equal size; two-arg form lets you make
  RX tiny (we used `dma_buffers!(2048, 16384)`).
- **DMA buffers come from internal RAM** (esp_alloc heap). 16 KiB TX + 2 KiB RX +
  descriptors ≈ 18 KiB of the ~144 KiB internal heap. There is no free lunch here.
- **Max single DMA transfer: 32,736 bytes** (`MAX_DMA_SIZE`, line ~103). A full
  frame needs ≥ 4 transfers; `SpiDmaBus::write`/`write_async` chunk automatically
  through `tx_buf.capacity()`.
- **Async conversion:** `SpiDmaBus<'_, Blocking>::into_async()` →
  `SpiDmaBus<'_, Async>` (line ~2186). It calls `set_interrupt_handler(...)`
  itself (line ~1454), installing the esp-hal async handler on the **current
  core** — call it from the core that will own the bus (core 0 / `lcd_task`).
- **Async write:** `write_async(&[u8])` is an **inherent method** on
  `SpiDmaBus<'d, Async>` (line ~2254) — no trait import needed. It also exists as
  `embedded_hal_async::spi::SpiBus::write` (line ~2630). It does
  `wait_for_idle_async` → copy chunk into `tx_buf` → `start_dma_transfer` →
  `wait_for_idle_async().await` per chunk, with a `DropGuard` that cancels the
  transfer if the future is dropped mid-flight.
- **Blocking DMA write:** `SpiDmaBus<Blocking>::write` (line ~2409) is the
  blocking equivalent — DMA moves the data, but the CPU busy-waits
  (`wait_for_idle()`). It's what a sync `embedded_hal::spi::SpiBus` impl uses.
- **`wait_for_idle_async`** (line ~1574) waits on the `TransferDone` SPI
  interrupt. On ESP32/ESP32-S2 it additionally re-polls the busy flag after the
  interrupt fires (`#[cfg(any(esp32, esp32s2))]` branch). The interrupt must
  actually reach the executor — with esp-rtos this works the same way the wifi
  driver's interrupts do.
- **Sync layer over the DMA bus:** `SpiDmaBus<Blocking>` implements the sync
  `embedded_hal::spi::SpiBus`, and `&mut T` forwards `SpiDevice` — so a small
  `SpiDevice` wrapper can drive the gc9a01 init sequence through blocking DMA
  before converting the bus to `Async`. (Tried in v2; only relevant if you revisit
  DMA.)

## 5. gc9a01 / display-interface: everything is synchronous

- `SPIDisplayInterface` (gc9a01 crate) is a thin constructor for
  `display_interface_spi::SPIInterface<SPI, DC>`, which requires
  `SPI: embedded_hal::spi::SpiDevice` (blocking) and `DC: OutputPin`.
- `display_interface::WriteOnlyDataCommand` is a **sync trait** — there is no
  async variant. `display.init()` / `display.clear()` can only run over a sync
  device.
- Practical consequence: a fully async display driver means reimplementing the
  gc9a01 init sequence with manual command bytes, or keeping a sync path for
  init and a separate async path for bulk pixels — neither is worth it given §3.
- Command bytes (used by the manual `flush_frame` in v2):
  `ColumnAddressSet = 0x2A + [sc_hi, sc_lo, ec_hi, ec_lo]`,
  `RowAddressSet = 0x2B + [sp_hi, sp_lo, ep_hi, ep_lo]`,
  `MemoryWrite = 0x2C`. DC pin: low = command, high = data.
  (gc9a01 `command.rs`, `Command::to_bytes`.)

## 6. Quirks & gotchas catalog

- **`SPI_DISPLAY_INTERFACE` static + `core::mem::transmute` to `&mut` from
  core 1:** technically two live `&mut`s (lcd_task also transmutes it on core 0).
  Safe in practice because WASM sessions blank the screen before the guest starts,
  so `lcd_task` is parked at `LcdScreen::Blank` and the two never write the LCD
  concurrently. Keep that invariant if you touch this code. Documented in a
  SAFETY comment on `HardwareDisplayManager::signal_raw_frame`.
- **The declaration line of that static has bitten us twice**: it is
  `*mut SPIInterface<SpiExclusiveDevice<'_>, Output<'_>>` — two closing `>`
  before `=`, three before `()` on the `ptr::null_mut::<...>` line. Count your
  brackets.
- **PSRAM and DMA:** the LCD `BUFFER`, WASM frame slots, and the guest's canvas
  all live in PSRAM (`Vec::new_in(ExternalMemory)`, wasmi `psram-alloc`). Direct
  DMA from a PSRAM `&[u8]` needs cache flush/writeback before the transfer
  starts; staging through an internal-RAM `DmaTxBuf` sidesteps the issue entirely.
- **Per-frame WebSocket snapshot:** copying 115 KB into `BUFFER` every frame
  saturates core 0; the 250 ms throttle (`LAST_SCREEN_UPDATE`) is intentional.
  The WebSocket remote view is ~4 fps by design.
- **`select` + `Signal` wake from core 1:** `Signal<CriticalSectionRawMutex, ()>`
  is cross-core safe; the producer must coalesce wakes (`try_take().is_none()`
  before `signal()`, since `Signal::signal` asserts if already signaled). A
  coalesced wake is fine when the consumer drains all pending work after each
  wake.
- **The hexpansion poll task scans 6 I2C ports every 2 seconds** — the usual
  suspect for the "half-second stall" when core 0 gets overloaded. Any new
  core-0 work should assume it will be delayed by ~this task.

## 7. The abstraction that stuck

`app/src/platform/display.rs`:

```rust
fn signal_raw_frame(&self, buffer: &[u8]) -> Result<(), DisplayError>; // FRAME_BYTES bytes
```

- Firmware: `HardwareDisplayManager::signal_raw_frame` = direct blocking SPI write
  from the calling core + throttled `BUFFER` snapshot (`firmware/src/platform/display.rs`).
- Desktop: copies into `LCD_BUFFER` for the render loop (`desktop/src/platform/display.rs`).
- WASM hosts (firmware `HardwareWasmHost`, desktop `DesktopWasmHost`) call it from
  `set_lcd_buffer`; neither touches SPI or display statics anymore.

## 8. If you revisit DMA, do this

1. One contiguous DMA transfer per frame (single descriptor chain), not
   `tx_buf`-chunked `write_async`.
2. Batch the window commands into the same transfer or send them on the blocking
   path — never 7 separate async writes per frame.
3. Keep the transfer off core 0's blocking path; either accept core-1 blocking
   (current design) or use a true background DMA with completion interrupt and a
   per-frame ownership handoff.
4. Re-measure against the current direct-write implementation (the ~87 fps bus
   ceiling is the bar).
