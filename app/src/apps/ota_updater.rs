use crate::{
  apps::{AppAction, AppInput, AppRunContext, AppRunEvent, MenuApp, MenuAppContext, common::AppName},
  platform::{HttpEventChannel, Platform},
  protocol::{HttpEvent, HttpRequest},
  types::*,
  ui::progress::Progress,
};
use alloc::{format, vec::Vec};
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
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  /// Download the version manifest, but give up (return `Err`) if any input
  /// or event arrives while the request is in flight.
  async fn download_manifest(&mut self, ctx: &AppRunContext<'_, P>) -> Result<VersionInfo, ()> {
    let req = HttpRequest::new(format!(
      "{}/version.json",
      self.ctx.platform.config_manager().get_data().await.firmware_url,
    ));

    let http_client = self.ctx.platform.http_client().ok_or(())?;
    let channel = HttpEventChannel::new();
    let mut meta = None;
    let mut body = Vec::new();

    let downloaded = embassy_futures::select::select(
      join(http_client.request(req, &channel), async {
        loop {
          match channel.receive().await {
            HttpEvent::Meta(m) => meta = Some(m),
            HttpEvent::Chunk(chunk) => body.extend(chunk),
            HttpEvent::Done => break,
            HttpEvent::Error => return,
          }
        }
      }),
      ctx.next(),
    )
    .await;
    if matches!(downloaded, embassy_futures::select::Either::Second(_)) {
      return Err(());
    }

    let _meta = meta.ok_or(())?;
    serde_json::from_slice::<VersionInfo>(&body).map_err(|_| ())
  }

  /// Download + flash the firmware, but give up (return `Err`) if any input
  /// or event arrives while the transfer is in flight.
  async fn do_update(&mut self, version_info: VersionInfo, ctx: &AppRunContext<'_, P>) -> Result<(), ()> {
    let http_client = self.ctx.platform.http_client().ok_or(())?;

    let req = HttpRequest::new(format!(
      "{}/firmware.bin",
      self.ctx.platform.config_manager().get_data().await.firmware_url,
    ));

    let channel = HttpEventChannel::new();
    let offset = self.ctx.platform.ota_begin().await.map_err(|_| ())?;
    let mut flash_addr = offset;
    let mut bytes_written = 0u32;
    let mut progress = Progress::new(0, version_info.size);
    let platform = self.ctx.platform.clone();
    let display = self.ctx.platform.display_manager();
    let app_ctx = self.ctx.clone();

    let listen = async {
      loop {
        match channel.receive().await {
          HttpEvent::Meta(_) => {}
          HttpEvent::Chunk(chunk) => {
            let _ = platform.ota_write_chunk(flash_addr, &chunk).await;
            flash_addr += chunk.len() as u32;
            bytes_written += chunk.len() as u32;
            progress.set_done(bytes_written);
            let _ = display.signal(progress.render());
          }
          HttpEvent::Done => {
            if bytes_written == version_info.size {
              info!("Firmware download complete");
              let _ = platform.ota_commit().await;
              app_ctx.notify("Update complete", Icon40::Info).await;
              platform.software_reset().await;
              return Ok(());
            } else {
              error!("Size mismatch: got {bytes_written}, expected {}", version_info.size);
              app_ctx.notify("Size mismatch!", Icon40::Error).await;
              return Err(());
            }
          }
          HttpEvent::Error => {
            info!("do_update: Error");
            return Err(());
          }
        }
      }
    };

    let result = embassy_futures::select::select(join(http_client.request(req, &channel), listen), ctx.next()).await;
    match result {
      embassy_futures::select::Either::First((_, res)) => res,
      embassy_futures::select::Either::Second(_) => Err(()),
    }
  }
}

impl<P: Platform> MenuApp<P> for OtaUpdaterApp<P> {
  fn render(&self) -> LcdScreen {
    let current = self.ctx.platform.firmware_version();
    match &self.state.screen {
      Screen::Welcome => LcdScreen::Headline(Icon40::Info, format!("v{current}: Press B to check")),
      Screen::UpdatePrompt(version_info) => LcdScreen::Headline(Icon40::Info, format!("v{current} -> v{}?", version_info.version)),
    }
  }

  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction {
    loop {
      let event = ctx.next().await;
      if let Some(action) = event.exit_action() {
        return action;
      }
      let AppRunEvent::Input(AppInput::Button(hex)) = event else {
        continue;
      };
      match &self.state.screen {
        Screen::Welcome => {
          if let HexButton::HexB = hex {
            let version = match self.download_manifest(&ctx).await {
              Ok(version) => version,
              Err(()) => {
                self.ctx.notify("Connection Error!", Icon40::Error).await;
                continue;
              }
            };
            self.state.screen = Screen::UpdatePrompt(version);
          }
        }
        Screen::UpdatePrompt(version_info) => {
          if let HexButton::Fire = hex
            && self.do_update(version_info.clone(), &ctx).await.is_err()
          {
            self.state.screen = Screen::Welcome;
          }
        }
      }
      self.ctx.update_lcd(self.render());
    }
  }
}
