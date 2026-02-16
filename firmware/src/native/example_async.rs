use crate::{
  lib::{HexButton, HostIpcMessage, Icon40, LcdScreen},
  native::{
    NativeAppContext,
    common::{NativeApp, NativeAppName, make_http_request},
  },
  utils::http::HttpRequest,
};
use alloc::{
  format,
  string::{String, ToString as _},
};
use core::future::join;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, rwlock::RwLock};
use embassy_time::{Duration, Timer};
use esp_hal::time::Instant;
use esp_println::println;
use log::error;

pub struct ExampleNativeAsyncApp {
  ctx: NativeAppContext,
  state: RwLock<NoopRawMutex, ExampleNativeAppState>,
}

impl NativeAppName for ExampleNativeAsyncApp {
  fn app_name() -> &'static str {
    "Example Native App"
  }
}

#[derive(Default)]
struct ExampleNativeAppState {
  display: Option<String>,
  quit: bool,
}

impl ExampleNativeAsyncApp {
  pub fn new(ctx: NativeAppContext) -> Self {
    Self {
      ctx,
      state: RwLock::new(ExampleNativeAppState::default()),
    }
  }

  async fn read_file(&self) {
    let device = self.ctx.local_fs.read_text_file("device.jsn").unwrap();

    let mut state = timeout_result!(self.state.write(), 1_000, "Refresh: State Lock Timeout").unwrap();
    state.display = Some(device);
  }

  async fn refresh(&self, url: String) {
    println!("ExampleNativeApp.refresh()");

    let response = timeout!(
      make_http_request(&self.ctx, HttpRequest::new(url)),
      5_000,
      "HTTP Timeout"
    );

    let state = timeout_result!(self.state.write(), 1_000, "Refresh: State Lock Timeout");

    match state {
      Ok(mut state) => {
        match response {
          Ok(response) => {
            println!("BODY: {}", response.body);
            let display = response.body[0..24].to_string();
            state.display = Some(display);
          }
          Err(err) => {
            state.display = Some(format!("{err:?}"));
          }
        };
      }
      Err(err) => {
        error!("state lock: {err}");
      }
    };
  }
}

impl NativeApp for ExampleNativeAsyncApp {
  async fn app_main(&self) -> () {
    let task_1 = async {
      loop {
        let (_, host_ipc_msg) = self.ctx.receiver.receive().await;
        match host_ipc_msg {
          HostIpcMessage::Stop => {
            println!("ExampleNativeApp: Received STOP instruction");
            let mut state = timeout_result!(self.state.write(), 1_000, "Run: State Lock Timeout").unwrap();
            state.quit = true;
            return;
          }
          HostIpcMessage::HexButton(hex_button) => {
            println!("ExampleNativeApp: {hex_button:?}");
            match hex_button {
              HexButton::A => {
                self.read_file().await;
              }
              HexButton::B => {
                self.refresh("http://example.com".to_string()).await;
              }
              HexButton::C => {
                self.refresh("http://frogfind.com".to_string()).await;
              }
              HexButton::D => {
                self.refresh("http://1.1.1.1".to_string()).await;
              }
              HexButton::E => {
                self.refresh("http://google.com".to_string()).await;
              }
              HexButton::F => {
                let state = timeout_result!(self.state.write(), 10, "Run: State Lock Timeout");
                if let Ok(mut state) = state {
                  state.display = None;
                }
              }
            };
          }
          _ => {}
        };
      }
    };

    let task_2 = async {
      loop {
        Timer::after(Duration::from_millis(1)).await;

        let state = timeout_result!(self.state.read(), 1_000, "Render: State Lock Timeout");

        if let Ok(state) = state {
          if state.quit {
            return;
          }

          match state.display {
            Some(ref display) => {
              self.ctx.update_lcd(LcdScreen::Headline(Icon40::Info, display.clone()));
            }
            None => {
              let ms = Instant::now().duration_since_epoch().as_millis() as u32;
              self.ctx.update_lcd(LcdScreen::BoundedProgress(ms % 1024, 1024));
            }
          };
        }
      }
    };

    join!(task_1, task_2).await;
  }
}
