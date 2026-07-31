#![no_std]

extern crate alloc;

// ================================ Input ================================

use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HexButton {
  Up,
  Right,
  Fire,
  Down,
  Left,
  HexA,
  HexB,
  HexC,
  HexD,
  HexE,
  HexF,
  Touch01,
  Touch02,
  Touch03,
  Touch04,
  Touch05,
  Touch06,
  Touch07,
  Touch08,
  Touch09,
  Touch10,
  Touch11,
  Touch12,
}

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

// ================================ WASM IPC ================================

/// Messages sent from a WASM guest to the host over the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WasmIpcMessage {
  HttpRequest(HttpRequest),
}

/// Messages sent from the host to a WASM guest over the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostIpcMessage {
  HexButton(HexButton),
  HttpError,
  HttpResponseMeta(HttpResponseMeta),
  HttpResponseBody(Vec<u8>),
  HttpResponseComplete,
}
