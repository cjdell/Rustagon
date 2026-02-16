#[macro_use]

mod common;
mod config;
mod delete_file;
mod list_files;
mod ota;
mod read_file;
mod receive_file;
mod web_socket;
mod wifi_join;
mod wifi_scan;
mod write_file;

use crate::{
  lib::{DeviceState, HttpSender, WebSocketIncomingSender},
  tasks::{
    http::{
      common::{CustomNotFound, cors_options_response, html_app_response, redirect_home_response},
      config::{GetConfigHandler, SaveConfigHandler},
      delete_file::DeleteFileHandler,
      list_files::HandleFileList,
      ota::OtaUpdateHandler,
      read_file::ReadFileHandler,
      receive_file::ReceiveFileHandler,
      web_socket::WebSocketHandler,
      wifi_join::HandleWifiJoin,
      wifi_scan::HandleWifiScan,
      write_file::WriteFileHandler,
    },
    wifi::{ScanWatch, WifiCommandSender},
  },
  utils::local_fs::LocalFs,
};
use alloc::{boxed::Box, vec::Vec};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::Duration;
use esp_alloc::ExternalMemory;
use log::info;
use picoserve::{
  AppBuilder, AppRouter, Router, Server,
  response::WebSocketUpgrade,
  routing::{PathRouter, get, get_service, post, post_service},
};

struct AppProps {
  local_fs: LocalFs,
  device_state: DeviceState,
  sender: HttpSender,
  web_socket_incoming_sender: WebSocketIncomingSender,
  wifi_command_sender: WifiCommandSender,
  scan_signal: &'static ScanWatch,
}

impl AppProps {
  pub fn new(
    local_fs: LocalFs,
    device_state: DeviceState,
    sender: HttpSender,
    web_socket_incoming_sender: WebSocketIncomingSender,
    wifi_command_sender: WifiCommandSender,
    scan_signal: &'static ScanWatch,
  ) -> Self {
    Self {
      local_fs,
      device_state,
      sender,
      web_socket_incoming_sender,
      wifi_command_sender,
      scan_signal,
    }
  }
}

impl AppBuilder for AppProps {
  type PathRouter = impl PathRouter;

  fn build_app(self) -> Router<Self::PathRouter> {
    let device_state_1 = self.device_state.clone();
    let device_state_2 = self.device_state.clone();

    Router::from_service(CustomNotFound)
      .route("/", get(async || html_app_response()))
      .route("/emulator", get(async || html_app_response()))
      .route("/remote", get(async || html_app_response()))
      .route("/fs", get(async || html_app_response()))
      .route("/config", get(async || html_app_response()))
      .nest(
        "/api",
        Router::new()
          .route(
            "/config",
            get_service(GetConfigHandler::new(device_state_1)).post_service(SaveConfigHandler::new(device_state_2)),
          )
          .route(
            "/wifi",
            get_service(HandleWifiScan::new(self.wifi_command_sender, self.scan_signal))
              .post_service(HandleWifiJoin::new(self.device_state, self.wifi_command_sender))
              .options(async || cors_options_response()),
          )
          .route("/files", get_service(HandleFileList::new(self.local_fs.clone())))
          .route(
            "/file",
            get_service(ReadFileHandler::new(self.local_fs.clone(), self.sender))
              .post_service(WriteFileHandler::new(self.local_fs.clone(), self.sender))
              .delete_service(DeleteFileHandler::new(self.local_fs.clone()))
              .options(async || cors_options_response()),
          )
          .route(
            "/receive",
            post_service(ReceiveFileHandler::new(self.sender)).options(async || cors_options_response()),
          )
          .route(
            "/reboot",
            post(async || {
              esp_hal::system::software_reset();
              "Unreachable"
            })
            .options(async || cors_options_response()),
          )
          .route(
            "/ota",
            post_service(OtaUpdateHandler).options(async || cors_options_response()),
          )
          .route(
            "/ws",
            get(async move |upgrade: WebSocketUpgrade| {
              upgrade.on_upgrade(WebSocketHandler::new(self.web_socket_incoming_sender)).with_protocol("messages")
            })
            .options(async || cors_options_response()),
          ),
      )
      // Captive Portal stuff...
      .route("/generate_204", get(async || redirect_home_response()))
      .route("/hotspot-detect.html", get(async || redirect_home_response()))
      .route("/connecttest.txt", get(async || redirect_home_response()))
      .route("/redirect", get(async || redirect_home_response()))
  }
}

const WEB_TASK_POOL_SIZE: usize = 3;

static CONFIG: picoserve::Config = picoserve::Config::new(picoserve::Timeouts {
  start_read_request: Some(Duration::from_secs(300)),
  persistent_start_read_request: Some(Duration::from_secs(300)),
  read_request: Some(Duration::from_secs(300)),
  write: Some(Duration::from_secs(300)),
});

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
async fn web_task(id: usize, stack: Stack<'static>, app: &'static AppRouter<AppProps>) -> ! {
  info!("Starting Web Task...");

  let port = 80;

  let mut tcp_rx_buffer = Vec::new_in(ExternalMemory);
  tcp_rx_buffer.resize(8 * 1024, 0);
  let mut tcp_tx_buffer = Vec::new_in(ExternalMemory);
  tcp_tx_buffer.resize(8 * 1024, 0);
  let mut http_buffer = Vec::new_in(ExternalMemory);
  http_buffer.resize(8 * 1024, 0);

  Box::new_in(
    Server::new(app, &CONFIG, http_buffer.as_mut())
      .listen_and_serve(
        id,
        stack,
        port,
        tcp_rx_buffer.as_mut_slice(),
        tcp_tx_buffer.as_mut_slice(),
      )
      .await,
    ExternalMemory,
  )
  .into_never()
}

pub fn start_http(
  spawner: Spawner,
  stack: Stack<'static>,
  local_fs: LocalFs,
  device_state: DeviceState,
  sender: HttpSender,
  web_socket_incoming_sender: WebSocketIncomingSender,
  wifi_command_sender: WifiCommandSender,
  scan_signal: &'static ScanWatch,
) {
  let app = mk_static!(
    AppRouter<AppProps>,
    AppProps::new(
      local_fs,
      device_state,
      sender,
      web_socket_incoming_sender,
      wifi_command_sender,
      scan_signal
    )
    .build_app()
  );

  for id in 0..WEB_TASK_POOL_SIZE {
    spawner.must_spawn(web_task(id, stack, app));
  }
}
