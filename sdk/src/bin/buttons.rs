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
use crate::lib::protocol::HostIpcMessage;
use crate::lib::tasks::get_next_host_message;
use crate::lib::tasks::spawn;
use alloc::boxed::Box;
use alloc::format;
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

    println!("Buffer created");

    let mut display = BufferTarget::new(buf);

    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    loop {
      match get_next_host_message().await.1 {
        HostIpcMessage::HexButton(hex) => {
          println!("HEX BUTTON: {hex:?}");

          let str = format!("You Pressed: {hex:?}");

          display.clear();

          let mut text = Text::new(&str, Point::new(55, 120 - 10), text_style);
          text.text_style.baseline = Baseline::Top;
          text.draw(&mut display).unwrap();

          set_lcd_buffer(display.get_buffer_ptr())
        }
        _ => {}
      }
    }
  })());
}
