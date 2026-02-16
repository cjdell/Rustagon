// Render some random shapes and text and also listen for button presses

#![no_std]
#![no_main]
#![feature(future_join)]
#![feature(thread_local)]

#[path = "../lib/mod.rs"]
#[macro_use]
mod lib;

extern crate alloc;

use crate::lib::{
  graphics::BufferTarget,
  helper::get_millis,
  protocol::extern_set_lcd_buffer,
  tasks::{HOST_IPC_CHANNEL, spawn, yield_now},
};
use alloc::{boxed::Box, format, string::ToString};
use embedded_graphics::{
  Drawable as _,
  mono_font::{MonoTextStyle, iso_8859_3::FONT_10X20},
  pixelcolor::Rgb565,
  prelude::{Point, Primitive as _, RgbColor, Size},
  primitives::{Circle, Line, PrimitiveStyle, Rectangle},
  text::Text,
};

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let buf = Box::new([0x00u8; 240 * 240 * 2]);

    println!("Buffer created");

    let mut display = BufferTarget::new(buf);

    let line_style = PrimitiveStyle::with_stroke(Rgb565::BLUE, 1);
    let fill_style = PrimitiveStyle::with_fill(Rgb565::YELLOW);
    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    println!("Drawing shapes...");

    let mut str = "Hello World!".to_string();

    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "subscriber");

    let start = get_millis();
    let mut last_tick = start;

    loop {
      let now = get_millis();

      let delta = now - last_tick;
      let elapsed = now - start;

      last_tick = now;

      // 5 second animation duration
      if now - start > 5_000 {
        break;
      }

      if let Some((_, host_ipc_msg)) = subscriber.try_next_message_pure() {
        str = format!("{host_ipc_msg:?}");
      }

      let i = (elapsed as i32) / 30;
      println!("delta: {delta}");

      display.clear();

      Circle::new(Point::new(i, 140), 48)
        .into_styled(fill_style)
        .draw(&mut display)
        .unwrap();

      Circle::new(Point::new(72, 8 + i), 48)
        .into_styled(line_style)
        .draw(&mut display)
        .unwrap();

      Line::new(Point::new(48, 16 + i), Point::new(8, 16 + i))
        .into_styled(line_style)
        .draw(&mut display)
        .unwrap();

      Line::new(Point::new(48, 16 + i), Point::new(64, 32 + i))
        .into_styled(line_style)
        .draw(&mut display)
        .unwrap();

      Rectangle::new(Point::new(79, 15 + i), Size::new(34, 34))
        .into_styled(line_style)
        .draw(&mut display)
        .unwrap();

      Text::new(&str, Point::new(5 + 1, 5 + i), text_style)
        .draw(&mut display)
        .unwrap();

      unsafe { extern_set_lcd_buffer(display.get_buffer_ptr()) };

      yield_now().await;
    }

    println!("Done writing to LCD");
  })());
}
