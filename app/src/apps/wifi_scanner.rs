use crate::{
  apps::{common::AppName, AppAction, MenuApp, MenuAppInput, MenuAppContext},
  platform::Platform,
  types::{WifiResult, *},
  utils::sleep,
};
use alloc::{format, string::String, string::ToString, vec::Vec};
use log::info;

pub struct WifiScannerApp<P: Platform> {
  ctx: MenuAppContext<P>,
  state: AppState,
}

impl<P: Platform> AppName for WifiScannerApp<P> {
  fn app_name() -> &'static str {
    "WiFi Scanner"
  }
}

enum Screen { Scanning, NetworkList }

struct AppState {
  screen: Screen,
  networks: Vec<WifiResult>,
  cursor: usize,
  status_message: Option<String>,
}

impl AppState {
  fn new() -> Self { Self { screen: Screen::Scanning, networks: Vec::new(), cursor: 0, status_message: None } }
}

impl<P: Platform> WifiScannerApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self { Self { ctx, state: AppState::new() } }

  async fn refresh_scan(&mut self) {
    self.state.screen = Screen::Scanning;
    self.state.networks = self.ctx.platform.wifi_manager().scan().await.unwrap_or_default();
    self.state.screen = Screen::NetworkList;
    self.state.cursor = 0;
    self.state.status_message = None;
  }

  async fn set_status(&mut self, msg: String) {
    self.state.status_message = Some(msg);
    self.ctx.update_lcd(self.render());
    sleep(2_000).await;
    self.state.status_message = None;
  }

  fn signal_bars(rssi: i8) -> &'static str {
    if rssi > -50 { "XXXX" } else if rssi > -60 { "XXX " } else if rssi > -70 { "XX  " } else { "X   " }
  }

  async fn connect_to_network(&mut self, ssid: &str) {
    let known_networks = self.ctx.platform.config_manager().get_data().await.known_wifi_networks;
    let known = known_networks.iter().find(|n| n.ssid == ssid);
    match known {
      Some(_) => {
        info!("Connecting to known network: {}", ssid);
        self.ctx.platform.wifi_manager().set_desired_state(crate::types::WifiDesiredState::Online).await;
        self.set_status(format!("Connecting to {}...", ssid)).await;
      }
      None => {
        info!("No saved password for: {}", ssid);
        self.set_status("No saved password".to_string()).await;
      }
    }
  }
}

impl<P: Platform> MenuApp for WifiScannerApp<P> {
  fn render(&self) -> LcdScreen {
    match &self.state.screen {
      Screen::Scanning => LcdScreen::Progress("Scanning WiFi...".to_string()),
      Screen::NetworkList => {
        if let Some(msg) = &self.state.status_message {
          LcdScreen::Headline(Icon40::Info, msg.clone())
        } else {
          let menu: Vec<MenuLine> = self.state.networks.iter()
            .map(|net| MenuLine(Icon20::Wifi, format!("{} {}", Self::signal_bars(net.signal_strength), net.ssid)))
            .collect();
          LcdScreen::Menu { menu, selected: self.state.cursor as u32 }
        }
      }
    }
  }

  async fn init(&mut self) {
    self.refresh_scan().await;
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    match input {
      MenuAppInput::Stop => AppAction::Stop,
      MenuAppInput::Button(hex) => {
        match &self.state.screen {
          Screen::Scanning => {}
          Screen::NetworkList => {
            self.state.status_message = None;
            match hex {
              HexButton::Up => { if self.state.cursor > 0 { self.state.cursor -= 1; } }
              HexButton::Down => { if self.state.cursor + 1 < self.state.networks.len() { self.state.cursor += 1; } }
              HexButton::Fire => {
                if let Some(name) = self.state.networks.get(self.state.cursor).map(|n| n.ssid.clone()) {
                  self.connect_to_network(&name).await;
                }
              }
              HexButton::Right => {
                self.refresh_scan().await;
              }
              _ => {}
            }
          }
        }
        AppAction::Continue
      }
    }
  }
}
