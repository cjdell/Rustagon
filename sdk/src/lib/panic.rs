use core::panic::PanicInfo;

// Minimal non-formatting panic handler. Keeps the guest small by avoiding
// core::fmt machinery on the panic path (std's panic handler is not linked).
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
  loop {}
}
