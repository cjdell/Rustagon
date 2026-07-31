extern crate alloc;
extern crate core;

#[link(wasm_import_module = "index")]
unsafe extern "C" {
  pub fn extern_write_stdout(str: *const u8, len: u32) -> ();
  pub fn extern_set_gpio(pin: i32, val: i32) -> ();
  pub fn extern_set_lcd_buffer(buf: *const u8) -> ();

  pub fn extern_register_timer(ms: u32) -> i32;
  pub fn extern_check_timer(id: i32) -> i32;

  pub fn extern_get_millis() -> u32;

  pub fn extern_write_wasm_ipc_message(buf: *const u8, len: u32) -> u32;
  pub fn extern_read_host_ipc_message(host_msg_id: u32, buf: *const u8) -> ();
}

// Wire protocol shared with the host runtime (app/firmware/desktop). The
// SDK only ever sees the wire-facing message variants — host-internal
// start/stop/lifecycle messages live in the app crate.
pub use wasm_protocol::*;
