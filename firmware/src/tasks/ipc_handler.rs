use crate::{
  platform::{HardwarePlatform, Platform},
  protocol::*,
  types::*,
};
use app::menu::state::{StackEvent, StackEntryType, StackEventHandle};
use app::platform::HttpEventChannel;
use app::protocol::HttpEvent;
use core::future::join;
use embassy_futures::select::{Either, select};
use log::info;
use wasm_protocol::{HostIpcMessage as WireHostIpcMessage, WasmIpcMessage as WireWasmIpcMessage};

#[embassy_executor::task]
pub async fn ipc_handler_task(
  wasm_ipc_channel: &'static WasmIpcChannel,
  http_event_receiver: HttpReceiver,
  host_ipc_sender: HostIpcSender,
  stack_event_handle: StackEventHandle,
  platform: HardwarePlatform,
) {
  info!("Starting IPC Handler Task...");

  loop {
    match select(
      wasm_ipc_channel.receive(),
      http_event_receiver.receive(),
    )
    .await
    {
      Either::First((wasm_req_id, wasm_ipc_message)) => {
        handle_wasm_message(wasm_req_id, wasm_ipc_message, &host_ipc_sender, &platform, &stack_event_handle).await;
      }
      Either::Second(http_message) => {
        handle_http_event(http_message, &host_ipc_sender, &stack_event_handle).await;
      }
    }
  }
}

async fn handle_wasm_message(
  wasm_req_id: u32,
  wasm_ipc_message: WasmIpcMessage,
  host_ipc_sender: &HostIpcSender,
  platform: &HardwarePlatform,
  stack_event_handle: &StackEventHandle,
) {
  match wasm_ipc_message {
    WasmIpcMessage::Started => {}
    WasmIpcMessage::MenuAppStarted => {}
    WasmIpcMessage::Stopped => {
      stack_event_handle.send(StackEvent::Popped);
    }
    WasmIpcMessage::LcdScreen(lcd_screen) => {
      let _ = platform.display_manager().signal(lcd_screen);
    }
    WasmIpcMessage::Wire(WireWasmIpcMessage::HttpRequest(http_request)) => {
      info!("IPC: HttpRequest from guest id={wasm_req_id} url={}", http_request.url);
      let http_client = match platform.http_client() {
        Some(client) => client,
        None => {
          info!("IPC: no http client, sending HttpError");
          host_ipc_sender.send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpError))).await;
          return;
        }
      };

      let channel = HttpEventChannel::new();
      join!(
        http_client.request(http_request, &channel),
        async {
          loop {
            match channel.receive().await {
              HttpEvent::Meta(meta) => {
                info!("IPC: forwarding meta to guest id={wasm_req_id} status={}", meta.status);
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpResponseMeta(meta)))).await;
              }
              HttpEvent::Chunk(chunk) => {
                info!("IPC: forwarding chunk to guest id={wasm_req_id} len={}", chunk.len());
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpResponseBody(chunk)))).await;
              }
              HttpEvent::Done => {
                info!("IPC: forwarding complete to guest id={wasm_req_id}");
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpResponseComplete))).await;
                break;
              }
              HttpEvent::Error => {
                info!("IPC: http error, forwarding HttpError to guest id={wasm_req_id}");
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::Wire(WireHostIpcMessage::HttpError))).await;
                break;
              }
            }
          }
        },
      )
      .await;
    }
  }
}

async fn handle_http_event(
  http_message: HttpStatusMessage,
  host_ipc_sender: &HostIpcSender,
  stack_event_handle: &StackEventHandle,
) {
  if let HttpStatusMessage::ReceivedFile(buffer) = http_message {
    stack_event_handle.send(StackEvent::Pushed(StackEntryType::HostedApp));
    host_ipc_sender.send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartWasmWithBuffer(buffer)))).await;
  }
}
