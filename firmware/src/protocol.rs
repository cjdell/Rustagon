use crate::{
  types::*,
  utils::http::{HttpRequest, HttpResponseMeta},
};
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WasmIpcMessage {
  Started,
  MenuAppStarted,
  Stopped,
  LcdScreen(LcdScreen),
  HttpRequest(HttpRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
