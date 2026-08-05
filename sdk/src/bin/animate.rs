// Render some random shapes and text and also listen for button presses

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt::{print_str, print_u32},
  gfx::{Canvas, Point, Rect, Rgb565},
  helper::{get_millis, print_line},
  protocol::extern_set_lcd_buffer,
  tasks::{HOST_IPC_CHANNEL, spawn, yield_now},
};
use alloc::{boxed::Box, format, string::ToString};

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let mut buf = Box::new([0x00u8; 240 * 240 * 2]);
    let mut canvas = Canvas::new(&mut buf[..], 240, 240);

    print_line("Buffer created\n");

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
      print_str("delta: ");
      print_u32(delta);
      print_line("\n");

      canvas.clear(Rgb565::BLACK);

      canvas.fill_circle(Point::new(i, 140), 48, Rgb565::YELLOW);
      canvas.draw_circle(Point::new(72, 8 + i), 48, Rgb565::BLUE);
      canvas.draw_line(Point::new(48, 16 + i), Point::new(8, 16 + i), Rgb565::BLUE);
      canvas.draw_line(Point::new(48, 16 + i), Point::new(64, 32 + i), Rgb565::BLUE);
      canvas.draw_rect(Rect::new(79, 15 + i, 34, 34), Rgb565::BLUE);
      canvas.draw_text(&str, 6, 5 + i, Rgb565::WHITE, 2);

      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

      yield_now().await;
    }

    print_line("Done writing to LCD");
  })());
}
