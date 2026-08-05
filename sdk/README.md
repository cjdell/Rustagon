# Rustagon WASM SDK

Write apps that run on the Rustagon badge (ESP32-S3) and in the desktop
emulator. Apps are compiled to `wasm32-unknown-unknown` and executed by the
host runtime, which provides the display, input, clock, and HTTP via a small
set of `extern` host functions.

The SDK is a single Cargo package: a `no_std` library crate (`src/lib.rs`)
plus one binary per app (`src/bin/*.rs`). Every app links the lib crate, which
provides the async runtime, drawing, formatting, and the `#[global_allocator]`
/ `#[panic_handler]`.

## App skeleton

```rust
#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use lib::{
  gfx::{Canvas, Rgb565},
  protocol::extern_set_lcd_buffer,
  tasks::{spawn, yield_now},
};
use alloc::boxed::Box;

// Called by the host once per frame. Delegate to the runtime's tick to drive
// spawned tasks and drain host messages. (Omit this to supply a custom tick.)
#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let mut buf = Box::new([0u8; 240 * 240 * 2]);
    let mut canvas = Canvas::new(&mut buf[..], 240, 240);

    canvas.clear(Rgb565::BLACK);
    canvas.draw_text("hello badge", 8, 8, Rgb565::WHITE, 2);
    unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

    loop {
      yield_now().await;
    }
  })());
}
```

## Library modules (`sdk/src/`)

| Module | What it provides |
|---|---|
| `gfx` | Zero-dependency drawing: `Canvas` (raw RGB565 framebuffer), `Point`/`Rect`, `Rgb565`, line/rect/circle/triangle (outline + fill), `blit`, and a 5x7 bitmap font (`draw_text`, scaleable). Pure `core`. |
| `fmt` | Integer/hex formatting and printing without `alloc::format!` — `u32_to_str`, `append_u32`, `print_u32`, … so the heavy `core::fmt` machinery never gets linked. |
| `tasks` | Async runtime: `spawn`, `yield_now`, `runtime_tick`, `get_next_host_message`, and `HOST_IPC_CHANNEL` (button/message subscriptions). |
| `trig` | `fast_sin`, `fast_cos`, `fast_sqrt` — compact approximations (no libm). |
| `http` | `make_http_request` (streams the response body via host functions). |
| `helper` | Host-call wrappers + `println!`, `print_str`, `log_error!`, `print_and_panic!` macros. |
| `protocol` | `extern "C"` host functions + re-export of `wasm_protocol` (buttons, HTTP wire types). |
| `sleep` | `sleep(ms)` via host timers. |
| `allocator` | `lol_alloc` global allocator + `get_memory_usage()` / `get_memory_allocated()` / `get_memory_deallocated()` exports. |
| `panic` | Non-formatting panic handler. |

The framebuffer is RGB565 in big-endian byte order (the same layout the host
LCD expects), so `canvas.as_ptr()` can be passed straight to
`extern_set_lcd_buffer`.

## Keeping apps small

- Use `gfx` + `fmt`; don't pull in formatting machinery for integers.
- The lib is `no_std` and all deps are `default-features = false` — keep it
  that way. Adding a std-based crate drags `std` into every app.
- `f32::cos()`/`f32::sin()` won't compile without std — use `trig` functions.
- Reference sizes: `bare` ≈ 100 B, `barefill` ≈ 1 KB, `barecube` ≈ 3 KB,
  `lines` ≈ 26 KB, `3dcubes` ≈ 35 KB, `jpeg` ≈ 113 KB (the decoder dominates).

## Building & running

All build commands must run inside `nix develop` (see the repo `AGENTS.md`):

```sh
nix develop --command bash -c "just build_sdk"                # build all apps
nix develop --command bash -c "just build_wasm <name>"        # build one app
nix develop --command bash -c "just run_desktop_app <name>"   # run in the desktop emulator
```

`just build_wasm <name>` produces `sdk/wasm/<name>.wsm` and regenerates
`sdk/wasm/manifest.json`, which the app store and emulator use to discover
apps.
