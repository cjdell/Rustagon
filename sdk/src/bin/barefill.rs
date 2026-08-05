#![no_std]
#![no_main]

use sdk as lib;

extern crate alloc;

use alloc::boxed::Box;
use lib::protocol::extern_set_lcd_buffer;

#[unsafe(no_mangle)]
fn wasm_main() {
  let buf = Box::new([0xf0u8; 240 * 240 * 2]);

  unsafe { extern_set_lcd_buffer(buf.as_ptr()) };
}

#[unsafe(no_mangle)]
fn tick(_: u32, _: u32) -> u32 {
  1 // Finish
}
