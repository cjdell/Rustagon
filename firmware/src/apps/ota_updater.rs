use super::{AppName, MenuAppAsync, MenuAppContext, MenuAppInput};
use crate::{
  lib::{FIRMWARE_VERSION, HexButton, Icon40, LcdScreen},
  utils::{
    cpu_guard::CpuGuard,
    http::{HttpEvent, HttpRequest, perform_http_request, perform_http_request_channel},
    ota::Ota,
    sleep,
  },
};
use alloc::{format, string::ToString};
use core::future::join;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel};
use esp_hal::{
  peripherals::{CPU_CTRL, FLASH},
  system::CpuControl,
};
use esp_println::println;
use esp_storage::FlashStorage;
use log::{error, info};
use partitions_macro::partition_offset;
use serde::{Deserialize, Serialize};

const OTA_0_OFFSET: u32 = partition_offset!("ota_0");
const OTA_1_OFFSET: u32 = partition_offset!("ota_1");
const OTA_OFFSETS: [u32; 2] = [OTA_0_OFFSET, OTA_1_OFFSET];

pub struct OtaUpdaterApp {
  ctx: MenuAppContext,
  state: AppState,
}

impl AppName for OtaUpdaterApp {
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
    Self {
      screen: Screen::Welcome,
    }
  }
}

#[derive(Clone, Serialize, Deserialize)]
struct VersionInfo {
  version: u32,
  size: u32,
}

impl OtaUpdaterApp {
  pub fn new(ctx: MenuAppContext) -> Self {
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  fn render(&self) {
    let screen = match &self.state.screen {
      Screen::Welcome => LcdScreen::Headline(Icon40::Info, "Press C to check update".to_string()),
      Screen::UpdatePrompt(version_info) => {
        let current_version = FIRMWARE_VERSION.parse::<u32>().unwrap();

        if version_info.version > current_version {
          LcdScreen::Headline(
            Icon40::Info,
            format!("Upgrade from {} to {}?", FIRMWARE_VERSION, version_info.version),
          )
        } else {
          LcdScreen::Headline(Icon40::Info, format!("You are up-to-date"))
        }
      }
    };

    self.ctx.update_lcd(screen);
  }

  async fn download_manifest(&mut self) -> Result<VersionInfo, anyhow::Error> {
    let req = HttpRequest::new(format!(
      "{}/version.json",
      self.ctx.device_state.get_data().firmware_url,
    ));

    let res = perform_http_request(self.ctx.stack, req).await.map_err(|_| anyhow::anyhow!("HTTP err"))?;

    Ok(serde_json::from_slice::<VersionInfo>(&res.body)?)
  }

  async fn do_update(&mut self, version_info: VersionInfo) -> Result<(), ()> {
    let req = HttpRequest::new(format!(
      "{}/firmware.bin",
      self.ctx.device_state.get_data().firmware_url,
    ));

    let channel = Channel::<NoopRawMutex, HttpEvent, 1>::new();
    let request = perform_http_request_channel(self.ctx.stack, channel.sender(), &req);

    let mut storage = FlashStorage::new(unsafe { FLASH::steal() });
    let mut ota = Ota::new(&mut storage);

    let current_slot = ota.current_slot();
    info!("Current Slot: {:?}", current_slot);
    let new_slot = current_slot.next();
    info!("New Slot: {:?}", new_slot);

    let mut flash_addr = OTA_OFFSETS[new_slot.number()];
    let mut bytes_written = 0u32;

    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    let _cpu_guard = CpuGuard::new(&mut cpu_ctrl);

    let listen = async {
      loop {
        match channel.receive().await {
          HttpEvent::Meta(_) => {}
          HttpEvent::Chunk(chunk) => {
            println!("do_update: Writing chunk... {}", chunk.len());
            ota.write(flash_addr, &chunk).unwrap();
            flash_addr += chunk.len() as u32;
            bytes_written += chunk.len() as u32;
            self.ctx.update_lcd(LcdScreen::BoundedProgress(bytes_written, version_info.size));
          }
          HttpEvent::Done => {
            if bytes_written == version_info.size {
              println!("do_update: Done!");
              ota.set_current_slot(new_slot);
              self.ctx.update_lcd(LcdScreen::Headline(Icon40::Info, "Update complete".to_string()));
              sleep(1_000).await;
              esp_hal::system::software_reset();
              return Ok(());
            } else {
              error!("do_update: Size mismatch!");
              self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, "Size mismatch!".to_string()));
              sleep(1_000).await;
              return Err(());
            }
          }
        }
      }
    };

    let (request_result, listen_result) = join!(request, listen).await;
    if let Err(err) = request_result {
      return Err(err);
    };
    if let Err(err) = listen_result {
      return Err(err);
    };

    Ok(())
  }

  async fn handle_welcome_input(&mut self, input: HexButton) {
    if let HexButton::C = input {
      let version = match self.download_manifest().await {
        Ok(version) => version,
        Err(err) => {
          self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, format!("{err:?}")));
          sleep(1_000).await;
          return;
        }
      };

      self.state.screen = Screen::UpdatePrompt(version);
    }
  }

  async fn handle_update_status_input(&mut self, input: HexButton, version_info: VersionInfo) {
    if let HexButton::C = input {
      if let Err(_) = self.do_update(version_info).await {
        self.state.screen = Screen::Welcome;
      }
    }
  }
}

impl MenuAppAsync for OtaUpdaterApp {
  async fn work(&mut self) -> bool {
    loop {
      self.render();

      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => match &self.state.screen {
          Screen::Welcome => self.handle_welcome_input(input).await,
          Screen::UpdatePrompt(version_info) => self.handle_update_status_input(input, version_info.clone()).await,
        },
        MenuAppInput::Stop => {
          return false;
        }
        _ => {}
      }
    }
  }
}
