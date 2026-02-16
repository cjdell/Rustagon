// Draw some moving lines

#![no_std]
#![no_main]
#![feature(future_join)]
#![feature(thread_local)]

#[path = "../lib/mod.rs"]
#[macro_use]
mod lib;

extern crate alloc;

use crate::lib::{
  graphics::{BufferTarget, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::get_millis,
  protocol::extern_set_lcd_buffer,
  tasks::{spawn, yield_now},
};
use alloc::boxed::Box;
use embedded_graphics::{
  Drawable as _,
  pixelcolor::Rgb565,
  prelude::{Point, Primitive as _, RgbColor},
  primitives::{Line, PrimitiveStyle},
};

static ANIMATION_DURATION: usize = 5_000;

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);

    let mut display = BufferTarget::new(buf);

    let line_style = PrimitiveStyle::with_stroke(Rgb565::WHITE, 1);
    let start = get_millis();
    let mut last_tick = start;

    loop {
      let now = get_millis();

      let delta = now - last_tick;
      let elapsed = now - start;

      last_tick = now;

      // 5 second animation duration
      if now - start > ANIMATION_DURATION as u32 {
        break;
      }

      let i = (elapsed as i32) / (ANIMATION_DURATION / SCREEN_WIDTH) as i32;

      println!("delta: {delta}");

      display.clear();

      Line::new(Point::new(0, i), Point::new(SCREEN_WIDTH as i32 - 1, i))
        .into_styled(line_style)
        .draw(&mut display)
        .unwrap();

      Line::new(Point::new(i, 0), Point::new(i, SCREEN_HEIGHT as i32 - 1))
        .into_styled(line_style)
        .draw(&mut display)
        .unwrap();

      unsafe { extern_set_lcd_buffer(display.get_buffer_ptr()) };

      yield_now().await;
    }
  })());
}
