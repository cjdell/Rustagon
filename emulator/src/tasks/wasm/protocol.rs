use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpResponseMeta {
  pub status: u32,
  pub headers: Vec<(String, String)>,
}
