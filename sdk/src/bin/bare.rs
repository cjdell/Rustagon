// A bare minimal program that does nothing

#![no_std]
#![no_main]
#![feature(future_join)]
#![feature(thread_local)]

#[unsafe(no_mangle)]
fn wasm_main() {}

#[unsafe(no_mangle)]
fn tick(_: u32, _: u32) -> u32 {
  1 // Returning `1` means I have finished
}

#[panic_handler]
fn panic(_panic: &core::panic::PanicInfo<'_>) -> ! {
  loop {}
}

#[unsafe(no_mangle)]
fn get_memory_usage() -> usize {
  0
}
