// Draw some moving lines

#![no_std]
#![no_main]

use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt::{print_str, print_u32},
  gfx::{Canvas, Point, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::{get_millis, print_line},
  protocol::extern_set_lcd_buffer,
  tasks::{spawn, yield_now},
};
use alloc::boxed::Box;

static ANIMATION_DURATION: usize = 5_000;

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let mut buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);
    let mut canvas = Canvas::new(&mut buf[..], SCREEN_WIDTH, SCREEN_HEIGHT);

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

      print_str("delta: ");
      print_u32(delta);
      print_line("\n");

      canvas.clear(Rgb565::BLACK);

      canvas.draw_line(Point::new(0, i), Point::new(SCREEN_WIDTH as i32 - 1, i), Rgb565::WHITE);
      canvas.draw_line(Point::new(i, 0), Point::new(i, SCREEN_HEIGHT as i32 - 1), Rgb565::WHITE);

      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

      yield_now().await;
    }
  })());
}
