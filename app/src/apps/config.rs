use crate::{
  apps::{common::AppName, AppAction, MenuApp, MenuAppInput, MenuAppContext},
  platform::{Platform, WifiStatus},
  types::*,
  utils::sleep,
};
use alloc::{format, string::String, string::ToString, vec};
use log::warn;

pub struct ConfigApp<P: Platform> {
  ctx: MenuAppContext<P>,
  state: AppState,
}

impl<P: Platform> AppName for ConfigApp<P> {
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
    Self { cursor: 0, wifi_status: WifiStatus::Offline, free_space: 0, message: None }
  }
}

impl<P: Platform> ConfigApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx, state: AppState::new() }
  }

  async fn refresh_status(&mut self) {
    let wifi_status = self.ctx.platform.wifi_manager().get_status().await;
    let files = self.ctx.platform.storage_manager().list_files().await.unwrap_or_default();
    let used: u32 = files.iter().map(|f| f.size).sum();
    let partition_kb = 1024u32;
    self.state.free_space = partition_kb.saturating_sub(used / 1024);
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
        self.ctx.platform.wifi_manager().set_desired_state(WifiDesiredState::Online).await;
        self.set_message("WiFi enabled").await;
      }
      _ => {
        self.ctx.platform.wifi_manager().set_desired_state(WifiDesiredState::Offline).await;
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
        self.ctx.platform.software_reset().await;
      }
      crate::types::WifiMode::AccessPoint => {
        cfg.wifi_mode = crate::types::WifiMode::Station;
        self.ctx.platform.config_manager().set_data(cfg).await;
        self.ctx.platform.config_manager().save().await.unwrap();
        self.set_message("Switching to STA mode...").await;
        sleep(1_000).await;
        self.ctx.platform.software_reset().await;
      }
    };
  }

  async fn format_fs(&mut self) {
    self.ctx.update_lcd(LcdScreen::Headline(Icon40::Info, "Formatting...".to_string()));
    match self.ctx.platform.format_storage().await {
      Ok(()) => {
        warn!("Filesystem formatted! Rebooting...");
        self.ctx.update_lcd(LcdScreen::Headline(Icon40::Info, "Format Complete!".to_string()));
        sleep(2_000).await;
        self.ctx.platform.software_reset().await;
      }
      Err(err) => {
        warn!("Format Error: {err:?}");
        self.set_message("Format failed!").await;
      }
    }
  }
}

impl<P: Platform> MenuApp for ConfigApp<P> {
  fn render(&self) -> LcdScreen {
    if let Some(ref msg) = self.state.message {
      return LcdScreen::Headline(Icon40::Info, msg.clone());
    }

    let ip_str: String = match self.state.wifi_status {
      WifiStatus::Connected(ip) => format!("IP: {ip}"),
      WifiStatus::AccessPoint => "AP Mode".to_string(),
      WifiStatus::Offline => "Offline".to_string(),
      _ => "Disconnected".to_string(),
    };

    LcdScreen::Menu {
      menu: vec![
        MenuLine(Icon20::Info, ip_str),
        MenuLine(Icon20::Info, format!("Free: {}KB", self.state.free_space)),
        MenuLine(Icon20::Config, CONFIG_OPTIONS[0].label().to_string()),
        MenuLine(Icon20::Config, CONFIG_OPTIONS[1].label().to_string()),
        MenuLine(Icon20::Config, CONFIG_OPTIONS[2].label().to_string()),
        MenuLine(Icon20::Info, "<= Back".to_string()),
      ],
      selected: self.state.cursor as u32,
      animation: MenuAnimation::FromRight,
    }
  }

  async fn init(&mut self) {
    self.refresh_status().await;
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    match input {
      MenuAppInput::Stop => AppAction::Stop,
      MenuAppInput::Button(hex) => {
        let max = CONFIG_OPTIONS.len() + 1;
        match hex {
          HexButton::Up => { if self.state.cursor > 2 { self.state.cursor -= 1; } }
          HexButton::Down => { if self.state.cursor + 1 < max + 2 { self.state.cursor += 1; } }
          HexButton::Fire => {
            let action_idx = self.state.cursor.checked_sub(2);
            match action_idx {
              Some(n) if n < CONFIG_OPTIONS.len() => match CONFIG_OPTIONS[n] {
                ConfigOption::WifiToggle => self.toggle_wifi().await,
                ConfigOption::WifiMode => self.toggle_mode().await,
                ConfigOption::Format => self.format_fs().await,
              },
              Some(n) if n == CONFIG_OPTIONS.len() => return AppAction::Stop,
              _ => {}
            }
          }
          _ => {}
        }
        AppAction::Continue
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
