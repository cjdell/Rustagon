// Fill the screen with pixels

#![no_std]
#![no_main]

use crate::lib::{
  allocator::get_memory_usage,
  fmt::{print_str, print_u32},
  helper::print_line,
  protocol::extern_set_lcd_buffer,
  sleep::sleep,
  tasks::spawn,
};
use alloc::boxed::Box;

use sdk as lib;

extern crate alloc;

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    print_line("About to write to LCD...");
    sleep(1_000).await;

    let buf = Box::new([0xf0u8; 240 * 240 * 2]);

    print_str("MEM: ");
    print_u32(get_memory_usage() as u32);
    print_line("\n");

    unsafe { extern_set_lcd_buffer(buf.as_ptr()) };
    print_line("Done writing to LCD");

    sleep(1_000).await;
  })());
}
