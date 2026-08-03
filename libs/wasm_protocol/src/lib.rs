#![no_std]

extern crate alloc;

// ================================ Input ================================

use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HexButton {
  /// Down/press events — the base variants keep their original meaning.
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
  /// Up/release events — one per button, emitted when the button is released.
  UpReleased,
  RightReleased,
  FireReleased,
  DownReleased,
  LeftReleased,
  HexAReleased,
  HexBReleased,
  HexCReleased,
  HexDReleased,
  HexEReleased,
  HexFReleased,
  Touch01Released,
  Touch02Released,
  Touch03Released,
  Touch04Released,
  Touch05Released,
  Touch06Released,
  Touch07Released,
  Touch08Released,
  Touch09Released,
  Touch10Released,
  Touch11Released,
  Touch12Released,
}

impl HexButton {
  /// Returns the release event for this button. Press events map to their
  /// `*Released` counterpart; release events map to themselves (idempotent),
  /// so producers can call this on any event they hold.
  pub const fn released(self) -> HexButton {
    match self {
      HexButton::Up => HexButton::UpReleased,
      HexButton::Right => HexButton::RightReleased,
      HexButton::Fire => HexButton::FireReleased,
      HexButton::Down => HexButton::DownReleased,
      HexButton::Left => HexButton::LeftReleased,
      HexButton::HexA => HexButton::HexAReleased,
      HexButton::HexB => HexButton::HexBReleased,
      HexButton::HexC => HexButton::HexCReleased,
      HexButton::HexD => HexButton::HexDReleased,
      HexButton::HexE => HexButton::HexEReleased,
      HexButton::HexF => HexButton::HexFReleased,
      HexButton::Touch01 => HexButton::Touch01Released,
      HexButton::Touch02 => HexButton::Touch02Released,
      HexButton::Touch03 => HexButton::Touch03Released,
      HexButton::Touch04 => HexButton::Touch04Released,
      HexButton::Touch05 => HexButton::Touch05Released,
      HexButton::Touch06 => HexButton::Touch06Released,
      HexButton::Touch07 => HexButton::Touch07Released,
      HexButton::Touch08 => HexButton::Touch08Released,
      HexButton::Touch09 => HexButton::Touch09Released,
      HexButton::Touch10 => HexButton::Touch10Released,
      HexButton::Touch11 => HexButton::Touch11Released,
      HexButton::Touch12 => HexButton::Touch12Released,
      released => released,
    }
  }
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
    Self {
      method: HttpMethod::Get,
      url,
      headers: Vec::new(),
      body: Vec::new(),
    }
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
    Self {
      status,
      headers: Vec::new(),
    }
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
