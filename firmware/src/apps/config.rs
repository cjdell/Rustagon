use crate::{
  apps::{
    MenuAppAsync, MenuAppInput,
    common::{AppName, MenuAppContext},
  },
  platform::{Platform, WifiStatus},
  types::*,
  utils::sleep,
};
use alloc::{format, string::String, string::ToString, vec, vec::Vec};
use core::net::Ipv4Addr;
use log::{info, warn};

pub struct ConfigApp {
  ctx: MenuAppContext,
  state: AppState,
}

impl AppName for ConfigApp {
  fn app_name() -> &'static str {
    "Configuration"
  }
}

#[derive(Clone, Copy, PartialEq)]
enum ConfigOption {
  WifiToggle,
  WifiMode,
  Format,
}

const CONFIG_OPTIONS: &[ConfigOption] = &[
  ConfigOption::WifiToggle,
  ConfigOption::WifiMode,
  ConfigOption::Format,
];

struct AppState {
  cursor: usize,
  wifi_status: WifiStatus,
  free_space: u32,
  message: Option<String>,
}

impl AppState {
  fn new() -> Self {
    Self {
      cursor: 0,
      wifi_status: WifiStatus::Offline,
      free_space: 0,
      message: None,
    }
  }
}

impl ConfigApp {
  pub fn new(ctx: MenuAppContext) -> Self {
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  fn render(&self) -> LcdScreen {
    if let Some(ref msg) = self.state.message {
      return LcdScreen::Headline(Icon40::Info, msg.clone());
    }

    let ip_str: String = match self.state.wifi_status {
      WifiStatus::Connected(ip) => format!("IP: {ip}"),
      WifiStatus::AccessPoint => format!("AP Mode"),
      WifiStatus::Offline => "Offline".to_string(),
      _ => "Disconnected".to_string(),
    };

    let menu = vec![
      MenuLine(Icon20::Info, ip_str),
      MenuLine(Icon20::Info, format!("Free: {}KB", self.state.free_space)),
      MenuLine(Icon20::Config, CONFIG_OPTIONS[0].label().to_string()),
      MenuLine(Icon20::Config, CONFIG_OPTIONS[1].label().to_string()),
      MenuLine(Icon20::Config, CONFIG_OPTIONS[2].label().to_string()),
      MenuLine(Icon20::Info, "<= Back".to_string()),
    ];

    LcdScreen::Menu {
      menu,
      selected: self.state.cursor as u32,
    }
  }

  async fn refresh_status(&mut self) {
    let data = self.ctx.platform.config_manager().get_data().await;
    let wifi_status = self.ctx.platform.wifi_manager().get_status().await;

    // Rough free space estimate: list files and sum sizes, compare to partition
    let files = self.ctx.platform.storage_manager().list_files().await.unwrap_or_default();
    let used: u32 = files.iter().map(|f| f.size).sum();
    let partition_kb = 1024u32; // 1MB partition
    let used_kb = used / 1024;
    self.state.free_space = partition_kb.saturating_sub(used_kb);

    self.state.wifi_status = wifi_status;
  }

  async fn set_message(&mut self, msg: &str) {
    self.state.message = Some(msg.to_string());
    self.ctx.update_lcd(self.render());
    sleep(2_000).await;
    self.state.message = None;
  }

  async fn toggle_wifi(&mut self) {
    match self.state.wifi_status {
      WifiStatus::Offline | WifiStatus::NoNetworksFound | WifiStatus::Interrupted => {
        self.ctx.platform.wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Online).await;
        self.set_message("WiFi enabled").await;
      }
      _ => {
        self.ctx.platform.wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Offline).await;
        self.set_message("WiFi disabled").await;
      }
    }
  }

  async fn toggle_mode(&mut self) {
    let mut cfg = self.ctx.platform.config_manager().get_data().await;
    match cfg.wifi_mode {
      crate::types::WifiMode::Station => {
        cfg.wifi_mode = crate::types::WifiMode::AccessPoint;
        self.ctx.platform.config_manager().set_data(cfg).await;
        self.ctx.platform.config_manager().save().await.unwrap();
        self.set_message("Switching to AP mode...").await;
        sleep(1_000).await;
        esp_hal::system::software_reset();
      }
      crate::types::WifiMode::AccessPoint => {
        cfg.wifi_mode = crate::types::WifiMode::Station;
        self.ctx.platform.config_manager().set_data(cfg).await;
        self.ctx.platform.config_manager().save().await.unwrap();
        self.set_message("Switching to STA mode...").await;
        sleep(1_000).await;
        esp_hal::system::software_reset();
      }
    };
  }

  async fn format_fs(&mut self) {
    self.ctx.update_lcd(LcdScreen::Headline(Icon40::Info, "Formatting...".to_string()));
    match self.ctx.platform.storage_manager().format().await {
      Ok(()) => {
        warn!("Filesystem formatted! Rebooting...");
        self.ctx.update_lcd(LcdScreen::Headline(Icon40::Info, "Format Complete!".to_string()));
        sleep(2_000).await;
        esp_hal::system::software_reset();
      }
      Err(err) => {
        warn!("Format Error: {err:?}");
        self.set_message("Format failed!").await;
      }
    }
  }
}

impl MenuAppAsync for ConfigApp {
  async fn work(&mut self) -> bool {
    self.refresh_status().await;

    loop {
      self.ctx.update_lcd(self.render());

      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => {
          let max = CONFIG_OPTIONS.len() + 1; // options + Back
          match input {
            HexButton::Up => {
              if self.state.cursor > 2 {
                self.state.cursor -= 1;
              }
            }
            HexButton::Down => {
              if self.state.cursor + 1 < max + 2 {
                self.state.cursor += 1;
              }
            }
            HexButton::Fire => {
              let action_idx = self.state.cursor.checked_sub(2);
              match action_idx {
                Some(n) if n < CONFIG_OPTIONS.len() => match CONFIG_OPTIONS[n] {
                  ConfigOption::WifiToggle => self.toggle_wifi().await,
                  ConfigOption::WifiMode => self.toggle_mode().await,
                  ConfigOption::Format => self.format_fs().await,
                },
                Some(n) if n == CONFIG_OPTIONS.len() => return false, // Back
                _ => {} // Informational lines
              }
            }
            _ => {}
          }
        }
        MenuAppInput::Refresh => {
          self.refresh_status().await;
        }
        MenuAppInput::Stop => return false,
        _ => {}
      }
    }
  }
}

impl ConfigOption {
  fn label(&self) -> &'static str {
    match self {
      ConfigOption::WifiToggle => "Toggle WiFi",
      ConfigOption::WifiMode => "Toggle AP/STA",
      ConfigOption::Format => "Format FS",
    }
  }
}
