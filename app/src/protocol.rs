use alloc::string::String;
use alloc::vec::Vec;
use display_types::LcdScreen;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::{Channel, Receiver, Sender}};
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
pub type WasmIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type HostIpcChannel = Channel<CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
