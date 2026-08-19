use alloc::string::String;
use alloc::vec::Vec;
use display_types::LcdScreen;
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  channel::{Channel, Receiver, Sender},
};
use serde::{Deserialize, Serialize};

// ================================ Wire protocol ================================
// The wire-facing types (shared with the WASM SDK) live in `wasm_protocol`.
pub use wasm_protocol::{HexButton, HttpMethod, HttpRequest, HttpResponseMeta};

// ================================ HTTP types ================================

#[derive(Debug, Clone)]
pub enum HttpEvent {
  Meta(HttpResponseMeta),
  Chunk(Vec<u8>),
  Done,
  Error,
}

// ================================ WASM IPC ================================

/// Host-internal commands sent to the WASM/native runtime. These never cross
/// the wire to a WASM guest — the SDK is unaware of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostRuntimeCommand {
  StartWasm(String),
  StartWasmWithBuffer(Vec<u8>),
  StartNative(String),
  Stop,
}

/// Runtime view of messages from the WASM subsystem. `Wire` carries messages
/// that crossed the guest boundary; the remaining variants are host-internal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WasmIpcMessage {
  Wire(wasm_protocol::WasmIpcMessage),
  Started,
  MenuAppStarted,
  Stopped,
  LcdScreen(LcdScreen),
}

/// Runtime view of messages to the WASM subsystem. `Runtime` carries
/// host-internal commands; `Wire` carries messages for a guest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostIpcMessage {
  Runtime(HostRuntimeCommand),
  Wire(wasm_protocol::HostIpcMessage),
}

// Channel type aliases
pub type WasmIpcChannel = Channel<CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;

#[cfg(test)]
mod http_wire_tests {
  use super::*;
  use alloc::vec;

  /// The wire field names are part of the JSON contract shared with the WASM
  /// SDK — a guest-serialised request must deserialize on the host unchanged.
  #[test]
  fn post_request_wire_format() {
    let json = r#"{
      "method": "Post",
      "url": "http://127.0.0.1:8080/echo",
      "headers": [["Content-Type", "application/json"], ["X-Custom", "abc"]],
      "body": [1, 2, 3]
    }"#;
    let req: HttpRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.method, HttpMethod::Post);
    assert_eq!(req.url, "http://127.0.0.1:8080/echo");
    assert_eq!(
      req.headers,
      vec![
        ("Content-Type".into(), "application/json".into()),
        ("X-Custom".into(), "abc".into())
      ]
    );
    assert_eq!(req.body, vec![1u8, 2, 3]);
  }

  #[test]
  fn http_request_round_trips() {
    let mut req = HttpRequest::new("http://example.com/api".into());
    req = req.with_method(HttpMethod::Put);
    req.headers.push(("Authorization".into(), "Bearer tok".into()));
    req.body.extend_from_slice(b"hello body");

    let json = serde_json::to_string(&req).unwrap();
    let back: HttpRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.method, HttpMethod::Put);
    assert_eq!(back.url, req.url);
    assert_eq!(back.headers, req.headers);
    assert_eq!(back.body, req.body);
  }

  #[test]
  fn get_request_omitted_method_defaults_to_get() {
    // `method` has a serde default (Get); `headers` and `body` are required wire
    // fields (guests always serialize them, even when empty).
    let req: HttpRequest = serde_json::from_str(r#"{ "url": "http://example.com/", "headers": [], "body": [] }"#).unwrap();
    assert_eq!(req.method, HttpMethod::Get);
    assert!(req.headers.is_empty());
    assert!(req.body.is_empty());
  }

  #[test]
  fn response_meta_round_trips() {
    let mut meta = HttpResponseMeta::new(200);
    meta.headers.push(("Content-Type".into(), "text/plain".into()));

    let json = serde_json::to_string(&meta).unwrap();
    let back: HttpResponseMeta = serde_json::from_str(&json).unwrap();
    assert_eq!(back.status, 200);
    assert_eq!(back.headers, meta.headers);
  }
}
pub type WasmIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type HostIpcChannel = Channel<CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
