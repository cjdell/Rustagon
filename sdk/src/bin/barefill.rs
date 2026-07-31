#![no_std]
#![no_main]

use crate::protocol::extern_set_lcd_buffer;
use alloc::boxed::Box;

#[path = "../lib/protocol.rs"]
#[macro_use]
mod protocol;

#[path = "../lib/allocator.rs"]
mod allocator;

#[path = "../lib/panic.rs"]
mod panic;

extern crate alloc;

#[unsafe(no_mangle)]
fn wasm_main() {
  let buf = Box::new([0xf0u8; 240 * 240 * 2]);

  unsafe { extern_set_lcd_buffer(buf.as_ptr()) };
}

#[unsafe(no_mangle)]
fn tick(_: u32, _: u32) -> u32 {
  1 // Finish
}
