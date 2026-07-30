use crate::types::HexButton;
use alloc::string::String;
use alloc::vec::Vec;
use display_types::LcdScreen;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::{Channel, Receiver, Sender}};
use serde::{Deserialize, Serialize};

// ================================ HTTP types ================================

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HttpMethod {
  Get,
  Post,
  Put,
  Delete,
}

impl Default for HttpMethod {
  fn default() -> Self {
    Self::Get
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
  #[serde(default)]
  pub method: HttpMethod,
  pub url: String,
  pub headers: Vec<(String, String)>,
  pub body: Vec<u8>,
}

impl HttpRequest {
  pub fn new(url: String) -> Self {
    Self { method: HttpMethod::Get, url, headers: Vec::new(), body: Vec::new() }
  }

  pub fn with_method(mut self, method: HttpMethod) -> Self {
    self.method = method;
    self
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponseMeta {
  pub status: u32,
  pub headers: Vec<(String, String)>,
}

impl HttpResponseMeta {
  pub fn new(status: u32) -> Self {
    Self { status, headers: Vec::new() }
  }
}

#[derive(Debug, Clone)]
pub enum HttpEvent {
  Meta(HttpResponseMeta),
  Chunk(Vec<u8>),
  Done,
  Error,
}

// ================================ WASM IPC ================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WasmIpcMessage {
  Started,
  MenuAppStarted,
  Stopped,
  LcdScreen(LcdScreen),
  HttpRequest(HttpRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostIpcMessage {
  StartWasm(String),
  StartWasmWithBuffer(Vec<u8>),
  StartNative(String),
  Stop,
  HexButton(HexButton),
  HttpError,
  HttpResponseMeta(HttpResponseMeta),
  HttpResponseBody(Vec<u8>),
  HttpResponseComplete,
}

// Channel type aliases
pub type WasmIpcChannel = Channel<CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type HostIpcChannel = Channel<CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
