// Demo a HTTP request

#![no_std]
#![no_main]
#![feature(future_join)]
#![feature(thread_local)]

#[path = "../lib/mod.rs"]
#[macro_use]
mod lib;

extern crate alloc;

use crate::lib::graphics::BufferTarget;
use crate::lib::helper::set_lcd_buffer;
use crate::lib::http::make_http_request;
use crate::lib::protocol::HttpRequest;
use crate::lib::sleep::sleep;
use crate::lib::tasks::spawn;
use alloc::boxed::Box;
use alloc::string::ToString;
use embedded_graphics::Drawable as _;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::Point;
use embedded_graphics::prelude::RgbColor;
use embedded_graphics::text::Baseline;
use embedded_graphics::text::Text;

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let buf = Box::new([0x00u8; 240 * 240 * 2]);

    let mut display = BufferTarget::new(buf);

    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    let resp = make_http_request(HttpRequest::new("http://firmware.rustagon.chrisdell.info".to_string())).await;

    let mut text = Text::new(&resp.body, Point::new(0, 0), text_style);
    text.text_style.baseline = Baseline::Top;
    text.draw(&mut display).unwrap();

    set_lcd_buffer(display.get_buffer_ptr());

    sleep(2_000).await;
  })());
}
