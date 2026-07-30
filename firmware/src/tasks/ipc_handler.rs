use crate::{
  platform::{HardwarePlatform, Platform},
  protocol::*,
  types::*,
  utils::sleep,
};
use app::menu::state::AppState;
use alloc::{string::ToString, sync::Arc};
use app::platform::HttpEventChannel;
use app::protocol::HttpEvent;
use core::sync::atomic::Ordering;
use core::future::join;
use embassy_futures::select::{Either, select};
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  rwlock::RwLock,
};
use log::info;

#[embassy_executor::task]
pub async fn ipc_handler_task(
  wasm_ipc_channel: &'static WasmIpcChannel,
  http_event_receiver: HttpReceiver,
  host_ipc_sender: HostIpcSender,
  platform: HardwarePlatform,
  app_state: Arc<RwLock<CriticalSectionRawMutex, AppState>>,
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
        handle_wasm_message(wasm_req_id, wasm_ipc_message, &host_ipc_sender, &platform, &app_state).await;
      }
      Either::Second(http_message) => {
        handle_http_event(http_message, &host_ipc_sender).await;
      }
    }
  }
}

async fn handle_wasm_message(
  wasm_req_id: u32,
  wasm_ipc_message: WasmIpcMessage,
  host_ipc_sender: &HostIpcSender,
  platform: &HardwarePlatform,
  app_state: &Arc<RwLock<CriticalSectionRawMutex, AppState>>,
) {
  match wasm_ipc_message {
    WasmIpcMessage::Started => {
      *app_state.write().await = AppState::HostedApp;
    }
    WasmIpcMessage::MenuAppStarted => {
      *app_state.write().await = AppState::MenuApp;
    }
    WasmIpcMessage::Stopped => {
      if app::apps::common::WASM_LAUNCHING.swap(false, Ordering::Acquire) {
        *app_state.write().await = AppState::HostedApp;
      } else {
        *app_state.write().await = AppState::None;
        let _ = platform.display_manager().signal(LcdScreen::Headline(Icon40::Info, "App Terminated".to_string()));
        sleep(1_000).await;
      }
    }
    WasmIpcMessage::LcdScreen(lcd_screen) => {
      let _ = platform.display_manager().signal(lcd_screen);
    }
    WasmIpcMessage::HttpRequest(http_request) => {
      let http_client = match platform.http_client() {
        Some(client) => client,
        None => {
          host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpError)).await;
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
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpResponseMeta(meta))).await;
              }
              HttpEvent::Chunk(chunk) => {
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpResponseBody(chunk))).await;
              }
              HttpEvent::Done => {
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpResponseComplete)).await;
                break;
              }
              HttpEvent::Error => {
                host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpError)).await;
                break;
              }
            }
          }
        },
      );
    }
  }
}

async fn handle_http_event(
  http_message: HttpStatusMessage,
  host_ipc_sender: &HostIpcSender,
) {
  if let HttpStatusMessage::ReceivedFile(buffer) = http_message {
    app::apps::common::WASM_LAUNCHING.store(true, Ordering::Release);
    host_ipc_sender.send((0, HostIpcMessage::StartWasmWithBuffer(buffer))).await;
  }
}
