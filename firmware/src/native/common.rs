use crate::{
  lib::{HostIpcMessage, HostIpcReceiver, LcdScreen, WasmIpcMessage, WasmIpcSender},
  utils::{
    http::{HttpRequest, HttpResponseMeta},
    local_fs::LocalFs,
  },
};
use alloc::{
  string::{String, ToString as _},
  vec::Vec,
};
use core::str::from_utf8;
use esp_alloc::ExternalMemory;
use esp_hal::time::Instant;
use esp_println::println;
use log::{error, warn};

pub trait NativeAppName {
  fn app_name() -> &'static str;
}

pub trait NativeApp {
  async fn app_main(&self) -> ();
}

pub struct NativeAppContext {
  pub local_fs: LocalFs,
  pub sender: WasmIpcSender,
  pub receiver: HostIpcReceiver,
}

impl NativeAppContext {
  pub fn new(local_fs: LocalFs, sender: WasmIpcSender, receiver: HostIpcReceiver) -> Self {
    Self {
      local_fs,
      sender,
      receiver,
    }
  }

  pub fn update_lcd(&self, lcd_screen: LcdScreen) {
    if self.sender.is_full() {
      println!("update_lcd: Clear");
      self.sender.clear();
    }

    if let Err(err) = self.sender.try_send((0, WasmIpcMessage::LcdScreen(lcd_screen))) {
      warn!("update_lcd: Failed to send: {err:?}");
    }
  }
}

pub struct HttpResponse {
  pub meta: HttpResponseMeta,
  pub body: String,
}

pub async fn make_http_request(ctx: &NativeAppContext, req: HttpRequest) -> Result<HttpResponse, anyhow::Error> {
  let req_id = Instant::now().duration_since_epoch().as_millis() as u32;

  ctx.sender.send((req_id, WasmIpcMessage::HttpRequest(req))).await;

  let mut response_meta: Option<HttpResponseMeta> = None;
  let mut response_body = Vec::new_in(ExternalMemory);

  loop {
    let (res_id, host_ipc_msg) = ctx.receiver.receive().await;

    match host_ipc_msg {
      HostIpcMessage::HttpError => {
        return Err(anyhow::Error::msg("HttpError"));
      }
      HostIpcMessage::HttpResponseMeta(meta) => {
        if res_id != req_id {
          continue;
        }

        response_meta = Some(meta);
      }
      HostIpcMessage::HttpResponseBody(body) => {
        if res_id != req_id {
          continue;
        }

        for b in body {
          response_body.push(b);
        }
      }
      HostIpcMessage::HttpResponseComplete => {
        return Ok(HttpResponse {
          meta: response_meta.unwrap(),
          body: match from_utf8(&response_body) {
            Ok(body) => body.to_string(),
            Err(err) => {
              error!("make_http_request: Could not decode body: {err:?}");
              return Err(anyhow::anyhow!(err));
            }
          },
        });
      }
      _ => {}
    }
  }
}
