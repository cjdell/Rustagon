// Fill the screen with pixels

#![no_std]
#![no_main]
#![feature(future_join)]
#![feature(thread_local)]

use crate::lib::{
  allocator::get_memory_usage, helper::print_line, protocol::extern_set_lcd_buffer, sleep::sleep, tasks::spawn,
};
use alloc::boxed::Box;

#[path = "../lib/mod.rs"]
#[macro_use]
mod lib;

extern crate alloc;

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    print_line("About to write to LCD...");
    sleep(1_000).await;

    let buf = Box::new([0xf0u8; 240 * 240 * 2]);

    println!("MEM: {}", get_memory_usage());

    unsafe { extern_set_lcd_buffer(buf.as_ptr()) };
    print_line("Done writing to LCD");

    sleep(1_000).await;
  })());
}
