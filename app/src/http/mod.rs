#[macro_use]
pub mod common;
pub mod config;
pub mod delete_file;
pub mod list_files;
pub mod ota;
pub mod read_file;
pub mod receive_file;
pub mod web_socket;
pub mod wifi_join;
pub mod wifi_scan;
pub mod write_file;

use crate::platform::Platform;
use crate::platform::display::DisplayHandle;
use crate::platform::storage::StorageHandle;
use crate::types::{DeviceConfig, HttpSender, WebSocketIncomingSender};
use common::*;
use picoserve::Router;
use picoserve::response::WebSocketUpgrade;
use picoserve::routing::{PathRouter, get, get_service, post, post_service};

pub use picoserve;

/// Build the `/api/*` sub-router with all API handlers.
/// Firmware and desktop should nest this under their own root router.
pub fn build_api_router<P: Platform + 'static>(
  storage: StorageHandle,
  sender: HttpSender,
  web_socket_incoming_sender: WebSocketIncomingSender,
  display: DisplayHandle,
  platform: P,
) -> Router<impl PathRouter> {
  Router::new()
    .route(
      "/config",
      get_service(config::GetConfigHandler::new(platform.config_manager()))
        .post_service(config::SaveConfigHandler::new(platform.config_manager())),
    )
    .route(
      "/wifi",
      get_service(wifi_scan::HandleWifiScan::new(platform.clone()))
        .post_service(wifi_join::HandleWifiJoin::new(platform.config_manager(), platform.clone()))
        .options(async || cors_options_response()),
    )
    .route("/files", get_service(list_files::HandleFileList::new(storage.clone())))
    .route(
      "/file",
      get_service(read_file::ReadFileHandler::new(storage.clone(), sender.clone()))
        .post_service(write_file::WriteFileHandler::new(storage.clone(), sender.clone()))
        .delete_service(delete_file::DeleteFileHandler::new(storage.clone()))
        .options(async || cors_options_response()),
    )
    .route(
      "/receive",
      post_service(receive_file::ReceiveFileHandler::new(sender.clone())).options(async || cors_options_response()),
    )
    .route("/reboot", post(async || "OK").options(async || cors_options_response()))
    .route(
      "/ota",
      post_service(ota::OtaUpdateHandler::new(platform)).options(async || cors_options_response()),
    )
    .route(
      "/ws",
      get({
        let ws_sender = web_socket_incoming_sender;
        let ws_display = display;
        async move |upgrade: WebSocketUpgrade| {
          upgrade
            .on_upgrade(web_socket::WebSocketHandler::new(ws_sender.clone(), ws_display.clone()))
            .with_protocol("messages")
        }
      })
      .options(async || cors_options_response()),
    )
}

pub async fn sleep(ms: u64) {
  embassy_time::Timer::after(embassy_time::Duration::from_millis(ms)).await;
}
