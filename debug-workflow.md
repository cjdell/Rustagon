# On-Device Debug Workflow

How to debug the Rustagon badge **on real hardware** with a purely agentic loop:
drive the device over its **USB serial** console and its **WebSocket screen/input
channel** (the same channel the WebUI's `/remote` page uses), verify by scraping
and OCR'ing the 240×240 screen, and correlate everything with serial logs.

This workflow was verified live against a badge at `192.168.49.144` on
`/dev/cu.usbmodem1101` (see [Verification](#verification)).

---

## 1. The two channels

| Channel | Endpoint | What you get |
|---|---|---|
| USB serial | `/dev/cu.usbmodem1101` (macOS; `/dev/ttyACM0` on Linux) | Boot log, panics, WiFi/IP/mDNS status, WASM start/stop, hexpansion events. Level is **INFO** by default; see [Log level](#log-level). |
| WebSocket | `ws://<ip>/api/ws` (subprotocol `messages`) | The live screen: binary frames every **250 ms**. Also accepts **JSON button presses** that are injected into the platform input queues — indistinguishable from physical presses. |
| HTTP API | `http://<ip>/api/*` | Config, reboot, WiFi scan/join, file list/read/write/delete, OTA, file upload. |

The WebUI is served **by the device itself** at `http://<ip>/` — `/remote` is the
manual screen + buttons page, `/fs` the file browser, `/config` settings.
`web/src/lib/device/badge.ts` is the reference client for the WS protocol.

Device facts (this badge): IP `192.168.49.144` (Station mode), mDNS
`rustagon.local` (may not resolve from the host — use the IP). Display
240×240 monochrome. The root menu lists the 10 built-in apps + `Power Off`
(`additional_apps` is `&[]` in this build, so WASM apps are **not** in the menu).

## 2. The WebSocket protocol (exact)

- **URL:** `ws://<ip>/api/ws`, subprotocol `"messages"`. Connection errors are
  expected while the badge is booting — it serves the web UI itself, so the WS
  comes back on the same IP after reboot.
- **Device → client (screen):** one **binary** frame every 250 ms
  (`app/src/http/web_socket.rs`). Exactly `240*240/8 = 7200` bytes; each **bit**
  is one pixel, LSB first within each byte, bit set = lit pixel. Convert to
  RGB565 as `px ? 0xffff : 0x0000`.
- **Client → device (input):** JSON **text** messages (`WebSocketIncomingMessage`):
  - `{"HexButton":"Down"}` and `{"HexButton":"DownReleased"}` — press/release.
    Button names: `Up Down Left Right Fire`, `HexA..HexF`, `Touch01..Touch12`,
    each with a `*Released` twin (`libs/wasm_protocol/src/lib.rs`).
  - `{"SystemMessage":"BootButton"}` — the boot button (quit the foreground app,
    return to the menu).
  - Received by `websocket_input_forwarder_task` (`firmware/src/bin/rustagon.rs`)
    which calls `platform.input_manager().inject_button(..)` — **same event
    queue as physical buttons**, including debounce behaviour.

## 3. The debug tools

`tools/debug/` is a small Deno CLI (zero deps, Deno 2.x). Everything defaults
to `BADGE_HOST` env var or `192.168.49.144`.

**Run them from the Nix flake.** The tools need `deno` and `espflash`, both
provisioned by `flake.nix` (no system deps). Either work inside the dev shell
(`nix develop` / direnv) or prefix with `nix develop --command bash -c "..."`.

The ergonomic entry point is the `just` recipes (root `justfile`) — arguments
pass straight through to the underlying tool:

```sh
just debug_shot   [host] [prefix]            # screen -> <prefix>.png/.txt/.frame + hash
just debug_ocr    [host | saved-prefix]      # OCR a live screen or a saved <prefix>.frame
just debug_input  Down Fire                  # button presses (press+release); "boot" = BootButton
just debug_watch  --timeout 8                # exit 0 when the screen hash changes
just debug_record --dir D --timeout 8        # record every changed frame to D/ (+ changes.log)
just debug_logs   [port] --out serial.log    # espflash monitor -> timestamped log (hard-resets the badge)
just debug_reboot                            # POST /api/reboot (soft reboot, WS reconnects after)
```

Or call the tools directly (same defaults):

```sh
cd tools/debug
deno task shot  [host] [prefix]              # capture screen -> <prefix>.png/.txt/.frame + frame hash
deno task ocr   [host | prefix]              # OCR a live screen or a saved <prefix>.frame
deno task watch [host] --changed             # exit 0 when the screen hash changes (--expect-hash H, --timeout S)
deno task input [host] -- Down Fire          # send button presses (+automatic releases); "boot" = BootButton
deno task record [host] --dir D --timeout 8  # record every changed frame to D/ (+ changes.log)
deno task logs  [port] --out serial.log      # espflash monitor -> timestamped serial log (hard-resets the badge)
deno task reboot [host]                      # POST /api/reboot (soft reboot, WS reconnects after)
```

Output artifacts: `.png` (1-bit grayscale, viewable), `.txt` (ASCII art),
`.frame` (raw decoded 0/1 pixels for OCR offline).

**OCR** (`tools/debug/lib/ocr.ts`) reads both fonts the badge renders with —
the menu's embedded-graphics `FONT_10X20` and the SDK's 5×7 — and handles the
inverted selection bar (black text on the blinking white bar). It prints lines
as `INV 10x20 y=120 cost=0  Input Test`. The font tables are generated by
`scripts/gen_fonts.py` — **pure Python stdlib** (PNG decoded with `zlib`, no
PIL); only regenerate if a font changes.

## 4. The workflow

### Step 0 — sanity

```sh
curl -s http://192.168.49.144/api/config      # device alive? (returns JSON)
ls /dev/cu.usbmodem*                           # serial present?
```

### Step 1 — start the serial log (background)

```sh
just debug_logs /dev/cu.usbmodem1101 --out /tmp/dbg/serial.log
# (or: cd tools/debug && deno task logs /dev/cu.usbmodem1101 --out /tmp/dbg/serial.log)
```

This runs `espflash monitor --non-interactive` which **hard-resets the badge**,
so you get a clean boot sequence (bootloader, flash ID, memory, I2C scan, WiFi
connect, IP, mDNS, WASM start). Keep it running while you drive the device; the
file gets timestamped lines like `[11:06:10.787] INFO - Starting Menu Task...`.

### Step 2 — look at the screen

```sh
just debug_shot 192.168.49.144 /tmp/dbg/s0
just debug_ocr  /tmp/dbg/s0
```

The OCR text IS the assertion target: e.g. the root menu reads
`App Store / Configuration / Editor / ... / INV Input Test / ... / Power Off`
(the `INV` line is the current selection).

### Step 3 — drive the device

```sh
just debug_input 192.168.49.144 Down Fire       # navigate, then activate
just debug_input boot                           # quit app / back to menu
```

Each press is sent as press + 120 ms hold + release, matching the WebUI's
mouse-down/up semantics. The menu reacts only to press events (`Up/Down/Fire`
in `app/src/menu/mod.rs`), so `DownReleased` is harmlessly ignored.

### Step 4 — assert on the screen

```sh
just debug_watch 192.168.49.144 --changed --timeout 8    # exit 0 iff screen changed
just debug_shot ... && just debug_ocr ...                 # then read the new state
```

Use `record` when you need the full sequence of frames (e.g. an animation or a
slow transition): it writes one PNG per changed frame plus `changes.log`.

### Step 5 — correlate with serial

Every screen change should have a serial counterpart:

- Menu navigation → usually silent at INFO (expected).
- WASM app start → `Starting WASM on SECOND CORE...` (and, if built with
  `ESP_LOG=DEBUG`, `HTTP: request url=...` for guest HTTP calls).
- WiFi issues → `WiFi Status: ...` / `WiFi: Attempting connection...`.
- Panic/abort → the ESP panic handler prints the backtrace on the serial port
  (grep the log for `panicked` / `Backtrace`).

### Step 6 — recover

```sh
just debug_reboot 192.168.49.144       # soft reboot; WS reconnects, serial shows the new boot
just debug_logs ...                    # or restart the serial capture for a clean boot log
```

If the badge is wedged and the API is unresponsive, restart `just debug_logs`
(espflash's hard reset) or power-cycle it.

## 5. Verification

All of the following were executed live against the badge:

1. **Serial** — `espflash monitor` boots the device: ROM bootloader →
   ESP-IDF 2nd-stage → PSRAM init → I2C scan → LCD → filesystem → WiFi connect
   (`IP address obtained: 192.168.49.144`) → mDNS → `Starting WASM on SECOND
   CORE...` → menu tasks. Timestamped capture into `serial.log`.
2. **Screen scrape** — WS frames decode to a 240×240 bitmap at ~4 fps; root
   menu, Input Test app menu, and an SSH app screen all rendered as PNG + ASCII.
3. **Remote input** — `Down`/`Up` moved the menu selection exactly one item per
   press (highlight bar walked down/up, verified by frame hash + OCR); `Fire`
   launched the selected app; `BootButton` returned to the root menu;
   `{"SystemMessage":"BootButton"}` and the `websocket_input_forwarder_task`
   path were confirmed in source.
4. **OCR** — every menu line read at cost 0:
   - root menu: `App Store`, `Configuration`, `Editor`, `Files`, `Hexpansions`,
     `INV Input Test`, `Firmware Update`, `Power Info`, `SSH`, `WiFi Scanner`,
     `Power Off` (selected item flagged `INV`).
   - Input Test app: `INV Up`, `Down`, `Left`, `Right`, `Fire`, `Hex A` … `Hex E`.
   - The selected item's black-on-white-bar text (including a 2-char bar `Up`)
     and the 1-px misaligned text start were handled.
5. **Cross-channel** — `deno task reboot` (HTTP) produced a fresh boot sequence
   on the serial log seconds later; screen hash stayed stable while idle.

## 6. Troubleshooting

- **No WS frames / connection refused** — badge is booting, on the wrong
  network, or in AP mode with a different IP. Reboot and retry; check
  `curl /api/config`. The WS only exists while the web task is up.
- **Serial silent** — INFO level logs are sparse when idle; that's normal.
  Trigger something (navigate, reboot) and watch. For deeper logs rebuild the
  firmware with `ESP_LOG=DEBUG` (see below).
- **Serial port busy** — `espflash monitor` holds the port; kill it first
  (`pkill -f "espflash monitor"`).
- **OCR returns nothing / garbage** — the screen may be a WASM app drawing
  custom graphics (not text), or mid-animation. Re-shoot after settling, or use
  `--raw`/`.png`/`.txt` to look at the raw pixels.
- **Selection drifts after rapid presses** — each press is one menu step, but
  the menu re-renders with a 500 ms slide animation; capture after it settles
  (`shot` waits 300 ms; add a beat between presses).
- **`espflash` needs the port path** — on Linux it's usually `/dev/ttyACM0`;
  `logs.ts` auto-detects `cu.usbmodem*`/`ttyACM*`/`ttyUSB*` if no port given.

## 7. Implementation notes

- **Log level:** `esp_println::logger::init_logger_from_env()` reads `ESP_LOG`
  at **build** time (`firmware/.cargo/config.toml` sets `INFO`). Rebuild with
  `ESP_LOG=DEBUG` (via the `just` recipes' env sourcing) for per-request HTTP
  logs etc.
- **Frame format source:** `app/src/http/web_socket.rs`
  (`u16_bitmask_to_u8_slice`), decoder reference `web/src/lib/device/badge.ts`.
- **Input injection source:** `firmware/src/bin/rustagon.rs` →
  `websocket_input_forwarder_task`.
- **Menu geometry (for OCR/rendering):** `libs/display_renderer/src/lib.rs`:
  `MARGIN=40`, `ICON_WIDTH=20`, `CHAR_WIDTH=10`, `LINE_HEIGHT=20`. Each row is
  a hexagon icon (x 40–60) + 10×20 text starting at x 60. The selected row is
  black text on a blinking gray bar (blink period ≈ 3.14 s; gray always reads
  as white in the 1-bit WS stream, so the bar is "white with black text").
- **WASM apps aren't in the root menu** in the current firmware build
  (`additional_apps: &[]` in `firmware/src/tasks/menu/mod.rs`); the badge
  auto-starts one WASM app on the second core at boot.
