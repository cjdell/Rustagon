#![feature(future_join, type_alias_impl_trait, mpmc_channel)]

#[path = "../tasks/mod.rs"]
mod tasks;
#[path = "../utils/mod.rs"]
mod utils;

use crate::tasks::wasm::protocol::{HexButton, HostIpcMessage, HttpResponseMeta, WasmIpcMessage};
use crate::tasks::wasm::wasmi_runner;
use crate::utils::print_memory_usage;
use minifb::{Key, Scale, Window, WindowOptions};
use std::future::join;
use std::sync::Arc;
use std::sync::mpmc::channel;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::task;
use tokio::{task::yield_now, time::Duration, time::sleep};

pub fn __make_static<T: ?Sized>(t: &mut T) -> &'static mut T {
  unsafe { ::core::mem::transmute(t) }
}

const WIDTH: usize = 240;
const HEIGHT: usize = 240;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  print_memory_usage();

  let (wasm_ipc_sender, wasm_ipc_receiver) = channel::<(u32, Vec<u8>)>();
  let (_host_ipc_sender, host_ipc_receiver) = channel::<(u32, Vec<u8>)>();

  print_memory_usage();

  let _lcd_buffer: Arc<RwLock<Vec<u32>>> = Arc::new(RwLock::new(vec![0; WIDTH * HEIGHT]));

  let mut window_options = WindowOptions::default();
  window_options.scale = Scale::X4;
  let mut window = Window::new("Rust OLED", WIDTH, HEIGHT, window_options).unwrap();

  let host_ipc_sender = _host_ipc_sender.clone();

  let send_host_ipc_msg = move |wasm_req_id: u32, host_ipc_msg: HostIpcMessage| {
    // let host_msg_bytes = postcard::to_allocvec(&host_ipc_msg).unwrap();
    let host_msg_bytes = serde_json::to_vec(&host_ipc_msg).unwrap();
    println!("HOST: {}", std::str::from_utf8(&host_msg_bytes).unwrap());

    host_ipc_sender.send((wasm_req_id, host_msg_bytes.clone())).unwrap();
    host_ipc_sender.send((wasm_req_id, host_msg_bytes)).unwrap();
  };

  let decode_wasm_ipc_msg = |wasm_msg_bytes: Vec<u8>| -> WasmIpcMessage {
    // postcard::from_bytes(wasm_msg_bytes.as_slice()).unwrap()
    println!("WASM: {}", std::str::from_utf8(&wasm_msg_bytes).unwrap());
    serde_json::from_slice(wasm_msg_bytes.as_slice()).unwrap()
  };

  let lcd_buffer_1 = _lcd_buffer.clone();

  task::spawn_blocking(|| {
    wasmi_runner(lcd_buffer_1, wasm_ipc_sender, host_ipc_receiver);
  });

  let lcd_buffer_2 = _lcd_buffer.clone();

  let ipc_task = async {
    loop {
      if let Ok((wasm_msg_id, wasm_msg_bytes)) = wasm_ipc_receiver.try_recv() {
        match decode_wasm_ipc_msg(wasm_msg_bytes) {
          WasmIpcMessage::HttpRequest(http_request) => {
            let client = reqwest::Client::new();

            let mut request = client.get(http_request.url);

            for (key, value) in http_request.headers {
              request = request.header(key, value);
            }

            let response = request.send().await.unwrap();

            let status = response.status().as_u16() as u32;

            let mut meta = HttpResponseMeta {
              status,
              headers: Vec::new(),
            };

            for (name, value) in response.headers() {
              meta
                .headers
                .push((name.to_string(), value.to_str().unwrap().to_owned()));
            }

            send_host_ipc_msg(wasm_msg_id, HostIpcMessage::HttpResponseMeta(meta));

            let body = response.bytes().await.unwrap();

            send_host_ipc_msg(wasm_msg_id, HostIpcMessage::HttpResponseBody(body.into()));

            send_host_ipc_msg(wasm_msg_id, HostIpcMessage::HttpResponseComplete);
          }
        };
      }

      yield_now().await;
    }
  };

  let render_task = async {
    let mut last_time = SystemTime::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
      let time = SystemTime::now();
      let time_delta = time.duration_since(last_time).unwrap();

      last_time = time;

      if window.is_key_down(Key::Space) {}

      if window.is_key_down(Key::A) {
        send_host_ipc_msg(0, HostIpcMessage::HexButton(HexButton::A));
      }
      if window.is_key_down(Key::B) {
        send_host_ipc_msg(0, HostIpcMessage::HexButton(HexButton::B));
      }
      if window.is_key_down(Key::C) {
        send_host_ipc_msg(0, HostIpcMessage::HexButton(HexButton::C));
      }
      if window.is_key_down(Key::D) {
        send_host_ipc_msg(0, HostIpcMessage::HexButton(HexButton::D));
      }
      if window.is_key_down(Key::E) {
        send_host_ipc_msg(0, HostIpcMessage::HexButton(HexButton::E));
      }
      if window.is_key_down(Key::F) {
        send_host_ipc_msg(0, HostIpcMessage::HexButton(HexButton::F));
      }

      window
        .update_with_buffer(&lcd_buffer_2.read().await, WIDTH, HEIGHT)
        .unwrap();

      sleep(Duration::from_millis(20)).await;
    }
  };

  join!(ipc_task, render_task).await;

  Ok(())
}
