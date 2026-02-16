use crate::lib::protocol::{
  HostIpcMessage, WasmIpcMessage, extern_get_millis, extern_read_host_ipc_message, extern_set_gpio,
  extern_set_lcd_buffer, extern_write_stdout, extern_write_wasm_ipc_message,
};
use alloc::vec;

macro_rules! println {
    () => {
        print_line("\n");
    };

    ($($arg:tt)*) => {
        crate::lib::helper::print_line(&alloc::format!("{}\n", format_args!($($arg)*)));
    };
}

macro_rules! print_and_panic {
    () => {
        print_reachable("\n");
    };

    ($($arg:tt)*) => {
        crate::lib::helper::print_reachable(&alloc::format!("{}\n", format_args!($($arg)*)))
    };
}

macro_rules! log_error {
  ($invoke:expr, $msg:expr) => {
    match $invoke {
      Ok(res) => res,
      Err(err) => print_and_panic!("{}: {:?}", $msg, err),
    }
  };
}

pub fn print_line(str: &str) {
  unsafe { extern_write_stdout(str.as_ptr(), str.len() as u32) }
}

pub fn print_reachable(msg: &str) -> ! {
  print_line(&msg);
  unreachable!()
}

pub fn set_lcd_buffer(buf: *const u8) {
  unsafe { extern_set_lcd_buffer(buf) };
}

pub fn set_gpio(pin: i32, val: i32) {
  unsafe { extern_set_gpio(pin, val) };
}

pub fn get_millis() -> u32 {
  unsafe { extern_get_millis() }
}

pub fn receive_host_ipc_message(host_msg_id: u32, host_msg_size: u32) -> HostIpcMessage {
  let mut host_msg_bytes = vec![0u8; host_msg_size as usize];

  unsafe { extern_read_host_ipc_message(host_msg_id, host_msg_bytes.as_mut_ptr()) };

  // match postcard::from_bytes::<HostIpcMessage>(&host_msg_bytes) {
  //   Ok(host_msg) => host_msg,
  //   Err(err) => print_and_panic!("tick: Error receiving message: {err}"),
  // }

  match serde_json_core::from_slice::<HostIpcMessage>(&host_msg_bytes) {
    Ok((host_msg, _)) => host_msg,
    Err(err) => print_and_panic!("tick: Error receiving message: {err}"),
  }
}

pub fn send_wasm_ipc_message(wasm_ipc_message: WasmIpcMessage) -> u32 {
  // let wasm_msg_bytes = postcard::to_allocvec(&wasm_ipc_message).unwrap();

  let wasm_msg_bytes = serde_json::to_vec(&wasm_ipc_message).unwrap();

  unsafe { extern_write_wasm_ipc_message(wasm_msg_bytes.as_ptr(), wasm_msg_bytes.len() as u32) }
}
