//! Zero-dependency graphics for WASM apps.
//!
//! A `core`-only replacement for the `embedded_graphics` surface the SDK
//! currently uses: a raw RGB565 framebuffer ([`Canvas`]) with pixel, line,
//! rectangle, circle, triangle, blit and bitmap-text drawing. No `alloc`, no
//! external crates — the whole module is self-contained, so it keeps working
//! even if `embedded-graphics` (and friends) are eventually dropped from
//! `sdk/Cargo.toml`.
//!
//! The framebuffer is RGB565 in big-endian byte order — identical to the
//! layout the host LCD expects — so a canvas can be handed straight to
//! `extern_set_lcd_buffer`:
//!
//! ```ignore
//! let buf = Box::new([0u8; 240 * 240 * 2]);
//! let mut canvas = Canvas::new(&mut buf[..], 240, 240);
//! canvas.clear(Rgb565::BLACK);
//! canvas.fill_circle(Point::new(120, 120), 40, Rgb565::RED);
//! canvas.draw_text("hello", 8, 8, Rgb565::WHITE, 2);
//! unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };
//! ```

mod canvas;
mod font;

pub use canvas::{Canvas, Point, Rect, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH};
pub use font::{text_height, text_width};
