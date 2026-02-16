extern crate alloc;
extern crate core;

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum WasmIpcMessage {
  HttpRequest(HttpRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HostIpcMessage {
  HexButton(HexButton),
  HttpError,
  HttpResponseMeta(HttpResponseMeta),
  HttpResponseBody(Vec<u8>),
  HttpResponseComplete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HexButton {
  A,
  B,
  C,
  D,
  E,
  F,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpRequest {
  pub url: String,
  pub headers: Vec<(String, String)>,
}

impl HttpRequest {
  pub fn new(url: String) -> Self {
    Self {
      url,
      headers: Vec::new(),
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpResponseMeta {
  pub status: u32,
  pub headers: Vec<(String, String)>,
}
