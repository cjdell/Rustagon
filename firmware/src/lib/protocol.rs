use crate::utils::http::{HttpRequest, HttpResponseMeta};
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WasmIpcMessage {
  Started,
  MenuAppStarted,
  Stopped,
  LcdScreen(super::LcdScreen),
  HttpRequest(HttpRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HostIpcMessage {
  StartWasm(String),
  StartWasmWithBuffer(Vec<u8>),
  StartNative(String),
  Stop,
  HexButton(super::HexButton),
  HttpError,
  HttpResponseMeta(HttpResponseMeta),
  HttpResponseBody(Vec<u8>),
  HttpResponseComplete,
}
