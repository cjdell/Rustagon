#![no_std]
#![no_main]

use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt::print_str,
  gfx::{Canvas, Rgb565},
  helper::print_line,
  protocol::{HexButton, HostIpcMessage, extern_set_lcd_buffer},
  tasks::{get_next_host_message, spawn},
};
use alloc::boxed::Box;

fn hex_button_name(hex: HexButton) -> &'static str {
  match hex {
    HexButton::Up => "Up",
    HexButton::Down => "Down",
    HexButton::Left => "Left",
    HexButton::Right => "Right",
    HexButton::Fire => "Fire",
    HexButton::HexA => "HexA",
    HexButton::HexB => "HexB",
    HexButton::HexC => "HexC",
    HexButton::HexD => "HexD",
    HexButton::HexE => "HexE",
    HexButton::HexF => "HexF",
    HexButton::Touch01 => "Touch01",
    HexButton::Touch02 => "Touch02",
    HexButton::Touch03 => "Touch03",
    HexButton::Touch04 => "Touch04",
    HexButton::Touch05 => "Touch05",
    HexButton::Touch06 => "Touch06",
    HexButton::Touch07 => "Touch07",
    HexButton::Touch08 => "Touch08",
    HexButton::Touch09 => "Touch09",
    HexButton::Touch10 => "Touch10",
    HexButton::Touch11 => "Touch11",
    HexButton::Touch12 => "Touch12",
    _ => "Button",
  }
}

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

    loop {
      match get_next_host_message().await.1 {
        HostIpcMessage::HexButton(hex) => {
          print_str("HEX BUTTON: ");
          print_line(hex_button_name(hex));
          print_line("\n");

          canvas.clear(Rgb565::BLACK);

          // "You Pressed: <name>" without any formatting machinery
          let mut msg = [0u8; 48];
          let mut len = 0;
          for &c in b"You Pressed: " {
            msg[len] = c;
            len += 1;
          }
          for &c in hex_button_name(hex).as_bytes() {
            msg[len] = c;
            len += 1;
          }
          let text = core::str::from_utf8(&msg[..len]).unwrap();

          canvas.draw_text(text, 20, 110, Rgb565::WHITE, 2);

          unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };
        }
        _ => {}
      }
    }
  })());
}
