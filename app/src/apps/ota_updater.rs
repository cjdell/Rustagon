use crate::{
  apps::common::AppName,
  apps::{MenuAppAsync, MenuAppInput, MenuAppContext},
  platform::{HttpEventChannel, Platform},
  protocol::{HttpEvent, HttpRequest},
  types::*,
  utils::sleep,
};
use alloc::{format, string::ToString, vec::Vec};
use embassy_futures::join::join;
use log::{error, info};
use serde::{Deserialize, Serialize};

pub struct OtaUpdaterApp<P: Platform> {
  ctx: MenuAppContext<P>,
  state: AppState,
}

impl<P: Platform> AppName for OtaUpdaterApp<P> {
  fn app_name() -> &'static str {
    "Firmware Update"
  }
}

enum Screen {
  Welcome,
  UpdatePrompt(VersionInfo),
}

struct AppState {
  screen: Screen,
}

impl AppState {
  fn new() -> Self {
    Self { screen: Screen::Welcome }
  }
}

#[derive(Clone, Serialize, Deserialize)]
struct VersionInfo {
  version: u32,
  size: u32,
}

impl<P: Platform> OtaUpdaterApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx, state: AppState::new() }
  }

  fn render(&self) {
    let screen = match &self.state.screen {
      Screen::Welcome => LcdScreen::Headline(Icon40::Info, "Press C to check update".to_string()),
      Screen::UpdatePrompt(version_info) => {
        LcdScreen::Headline(Icon40::Info, format!("Update to v{}?", version_info.version))
      }
    };
    self.ctx.update_lcd(screen);
  }

  async fn download_manifest(&mut self) -> Result<VersionInfo, ()> {
    let req = HttpRequest::new(format!(
      "{}/version.json",
      self.ctx.platform.config_manager().get_data().await.firmware_url,
    ));

    let http_client = self.ctx.platform.http_client().ok_or(())?;
    let channel = HttpEventChannel::new();
    let mut meta = None;
    let mut body = Vec::new();

    join(
      http_client.request(req, &channel),
      async {
        loop {
          match channel.receive().await {
            HttpEvent::Meta(m) => meta = Some(m),
            HttpEvent::Chunk(chunk) => body.extend(chunk),
            HttpEvent::Done => break,
            HttpEvent::Error => return,
          }
        }
      },
    )
    .await;

    let _meta = meta.ok_or(())?;
    serde_json::from_slice::<VersionInfo>(&body).map_err(|_| ())
  }

  async fn do_update(&mut self, version_info: VersionInfo) -> Result<(), ()> {
    let http_client = self.ctx.platform.http_client().ok_or(())?;

    let req = HttpRequest::new(format!(
      "{}/firmware.bin",
      self.ctx.platform.config_manager().get_data().await.firmware_url,
    ));

    let channel = HttpEventChannel::new();
    let offset = self.ctx.platform.ota_begin().await.map_err(|_| ())?;
    let mut flash_addr = offset;
    let mut bytes_written = 0u32;
    let platform = self.ctx.platform.clone();
    let display = self.ctx.platform.display_manager();

    let listen = async {
      loop {
        match channel.receive().await {
          HttpEvent::Meta(_) => {}
          HttpEvent::Chunk(chunk) => {
            let _ = platform.ota_write_chunk(flash_addr, &chunk).await;
            flash_addr += chunk.len() as u32;
            bytes_written += chunk.len() as u32;
            let _ = display.signal(LcdScreen::BoundedProgress(bytes_written, version_info.size));
          }
          HttpEvent::Done => {
            if bytes_written == version_info.size {
              info!("Firmware download complete");
              let _ = display.signal(LcdScreen::Headline(Icon40::Info, "Update complete".to_string()));
              let _ = platform.ota_commit().await;
              sleep(1_000).await;
              platform.software_reset().await;
              return Ok(());
            } else {
              error!("Size mismatch: got {bytes_written}, expected {}", version_info.size);
              let _ = display.signal(LcdScreen::Headline(Icon40::Error, "Size mismatch!".to_string()));
              sleep(1_000).await;
              return Err(());
            }
          }
          HttpEvent::Error => {
            return Err(());
          }
        }
      }
    };

    join(http_client.request(req, &channel), listen).await;
    Ok(())
  }

  async fn handle_welcome_input(&mut self, input: HexButton) {
    if let HexButton::Fire = input {
      let version = match self.download_manifest().await {
        Ok(version) => version,
        Err(()) => {
          self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, "Connection Error!".to_string()));
          sleep(1_000).await;
          return;
        }
      };
      self.state.screen = Screen::UpdatePrompt(version);
    }
  }

  async fn handle_update_status_input(&mut self, input: HexButton, version_info: VersionInfo) {
    if let HexButton::Fire = input {
      if self.do_update(version_info).await.is_err() {
        self.state.screen = Screen::Welcome;
      }
    }
  }
}

impl<P: Platform> MenuAppAsync for OtaUpdaterApp<P> {
  async fn work(&mut self) -> bool {
    loop {
      self.render();
      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => match &self.state.screen {
          Screen::Welcome => self.handle_welcome_input(input).await,
          Screen::UpdatePrompt(version_info) => {
            self.handle_update_status_input(input, version_info.clone()).await
          }
        },
        MenuAppInput::Stop => return false,
        _ => {}
      }
    }
  }
}
