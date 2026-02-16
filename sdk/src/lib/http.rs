extern crate alloc;

use crate::lib::helper::send_wasm_ipc_message;
use crate::lib::protocol::{HostIpcMessage, HttpRequest, HttpResponseMeta, WasmIpcMessage};
use crate::lib::tasks::get_next_host_message;
use alloc::string::{String, ToString as _};
use alloc::vec::Vec;
use core::str::from_utf8;

pub struct HttpResponse {
  pub meta: HttpResponseMeta,
  pub body: String,
}

pub async fn make_http_request(req: HttpRequest) -> HttpResponse {
  let req_id = send_wasm_ipc_message(WasmIpcMessage::HttpRequest(req));

  let mut response_meta: Option<HttpResponseMeta> = None;
  let mut response_body: Vec<u8> = Vec::new();

  loop {
    match get_next_host_message().await {
      (res_id, HostIpcMessage::HttpResponseMeta(meta)) => {
        if res_id != req_id {
          continue;
        }

        response_meta = Some(meta);
      }
      (res_id, HostIpcMessage::HttpResponseBody(body)) => {
        if res_id != req_id {
          continue;
        }

        response_body.extend(body);
      }
      (res_id, HostIpcMessage::HttpResponseComplete) => {
        if res_id != req_id {
          continue;
        }

        return HttpResponse {
          meta: response_meta.unwrap(),
          body: match from_utf8(&response_body) {
            Ok(body) => body.to_string(),
            Err(err) => {
              print_and_panic!("make_http_request: Could not decode body: {err:?}")
            }
          },
        };
      }
      _ => {}
    }
  }
}
