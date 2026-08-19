use crate::platform::DesktopPlatform;
use app::http::common::{CustomNotFound, html_app_response};
use app::http::picoserve;
use app::platform::Platform;
use app::platform::storage::StorageHandle;
use app::types::{HttpSender, WebSocketIncomingSender};
use log::info;
use picoserve::{
  AppBuilder, AppRouter, Router,
  response::StatusCode,
  routing::{PathRouter, get},
};

/// Desktop equivalent of `firmware/src/tasks/http/mod.rs`. Runs the same
/// picoserve app (built by `app::http::build_api_router`) but on the tokio
/// runtime instead of the embassy runtime, one connection at a time on a
/// background thread.
pub const HTTP_PORT: u16 = 80;

static CONFIG: picoserve::Config = picoserve::Config::new(picoserve::Timeouts {
  start_read_request: picoserve::time::Duration::from_secs(300),
  persistent_start_read_request: picoserve::time::Duration::from_secs(300),
  read_request: picoserve::time::Duration::from_secs(300),
  write: picoserve::time::Duration::from_secs(300),
});

struct AppProps {
  storage: StorageHandle,
  sender: HttpSender,
  web_socket_incoming_sender: WebSocketIncomingSender,
  display: app::platform::display::DisplayHandle,
  platform: DesktopPlatform,
}

fn redirect_home_response() -> impl picoserve::response::IntoResponse {
  picoserve::response::Response::new(StatusCode::TEMPORARY_REDIRECT, "").with_headers([("Location", "/")])
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

/// Start the HTTP server on a background thread. Returns immediately; the
/// thread runs forever serving requests on [`HTTP_PORT`].
pub fn start_http(sender: HttpSender, web_socket_incoming_sender: WebSocketIncomingSender, platform: DesktopPlatform) {
  let storage = platform.storage_manager();
  let display = platform.display_manager();

  // Leak the router — it lives for the rest of the process (mirrors firmware's make_static!).
  let app: &'static AppRouter<AppProps> = Box::leak(Box::new(
    AppProps {
      storage,
      sender,
      web_socket_incoming_sender,
      display,
      platform,
    }
    .build_app(),
  ));

  std::thread::spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(serve(app));
  });
}

async fn serve(app: &'static AppRouter<AppProps>) {
  let addr = (std::net::Ipv4Addr::UNSPECIFIED, HTTP_PORT);
  let listener = match tokio::net::TcpListener::bind(addr).await {
    Ok(listener) => listener,
    Err(err) => {
      log::error!("HTTP server failed to bind 0.0.0.0:{HTTP_PORT}: {err}");
      return;
    }
  };

  info!("HTTP server listening on http://localhost:{HTTP_PORT}");

  tokio::task::LocalSet::new()
    .run_until(async {
      loop {
        let (stream, remote) = match listener.accept().await {
          Ok(connection) => connection,
          Err(err) => {
            log::warn!("HTTP accept error: {err}");
            continue;
          }
        };

        info!("HTTP connection from {remote}");

        tokio::task::spawn_local(async move {
          let mut http_buffer = vec![0u8; 8 * 1024];
          match picoserve::Server::new_tokio(app, &CONFIG, &mut http_buffer).serve(stream).await {
            Ok(disconnection) => {
              log::debug!("HTTP {remote}: handled {} requests", disconnection.handled_requests_count);
            }
            Err(err) => {
              log::warn!("HTTP {remote}: request error: {err:?}");
            }
          }
        });
      }
    })
    .await
}
