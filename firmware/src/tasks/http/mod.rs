use crate::{
  platform::Platform,
  platform::StorageHandle,
  types::*,
};
use alloc::{boxed::Box, vec::Vec};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::Duration;
use esp_alloc::ExternalMemory;
use log::info;
use picoserve::{
  AppBuilder, AppRouter, Router, Server, make_static,
  response::StatusCode,
  routing::{PathRouter, get},
};
use app::http::common::{cors_options_response, html_app_response, CustomNotFound};
use app::http::picoserve;

static CONFIG: picoserve::Config = picoserve::Config::new(picoserve::Timeouts {
  start_read_request: Some(Duration::from_secs(300)),
  persistent_start_read_request: Some(Duration::from_secs(300)),
  read_request: Some(Duration::from_secs(300)),
  write: Some(Duration::from_secs(300)),
});

struct AppProps {
  storage: StorageHandle,
  sender: HttpSender,
  web_socket_incoming_sender: WebSocketIncomingSender,
  display: app::platform::display::DisplayHandle,
  platform: crate::platform::HardwarePlatform,
}

fn redirect_home_response() -> impl picoserve::response::IntoResponse {
  picoserve::response::Response::new(StatusCode::TEMPORARY_REDIRECT, "")
    .with_headers([("Location", "/")])
}

impl AppBuilder for AppProps {
  type PathRouter = impl PathRouter;

  fn build_app(self) -> Router<Self::PathRouter> {
    let api_router = app::http::build_api_router(
      self.storage,
      self.sender,
      self.web_socket_incoming_sender,
      self.display,
      self.platform,
    );

    Router::from_service(CustomNotFound)
      .route("/", get(async || html_app_response()))
      .route("/emulator", get(async || html_app_response()))
      .route("/remote", get(async || html_app_response()))
      .route("/fs", get(async || html_app_response()))
      .route("/config", get(async || html_app_response()))
      .route("/generate_204", get(async || redirect_home_response()))
      .route("/hotspot-detect.html", get(async || redirect_home_response()))
      .route("/connecttest.txt", get(async || redirect_home_response()))
      .route("/redirect", get(async || redirect_home_response()))
      .nest("/api", api_router)
  }
}

const WEB_TASK_POOL_SIZE: usize = 3;

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
      .listen_and_serve(id, stack, port, tcp_rx_buffer.as_mut_slice(), tcp_tx_buffer.as_mut_slice())
      .await,
    ExternalMemory,
  )
  .into_never()
}

pub fn start_http(
  spawner: Spawner,
  stack: Stack<'static>,
  storage: StorageHandle,
  sender: HttpSender,
  web_socket_incoming_sender: WebSocketIncomingSender,
  platform: crate::platform::HardwarePlatform,
) {
  let display = platform.display_manager();

  let app = make_static!(
    AppRouter<AppProps>,
    AppProps {
      storage,
      sender,
      web_socket_incoming_sender,
      display,
      platform,
    }
    .build_app()
  );

  for id in 0..WEB_TASK_POOL_SIZE {
    spawner.must_spawn(web_task(id, stack, app));
  }
}
