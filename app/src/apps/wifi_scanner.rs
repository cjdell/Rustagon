use crate::{
  apps::{AppAction, MenuApp, MenuAppContext, MenuAppInput, common::AppName},
  platform::Platform,
  types::{WifiResult, *},
};
use alloc::{format, string::ToString, vec::Vec};
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

enum Screen {
  Scanning,
  NetworkList,
}

struct AppState {
  screen: Screen,
  networks: Vec<WifiResult>,
  cursor: usize,
}

impl AppState {
  fn new() -> Self {
    Self {
      screen: Screen::Scanning,
      networks: Vec::new(),
      cursor: 0,
    }
  }
}

impl<P: Platform> WifiScannerApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  async fn refresh_scan(&mut self) {
    self.state.screen = Screen::Scanning;
    self.state.networks = self.ctx.platform.wifi_manager().scan().await.unwrap_or_default();
    self.state.screen = Screen::NetworkList;
    self.state.cursor = 0;
  }

  fn signal_bars(rssi: i8) -> &'static str {
    if rssi > -50 {
      "XXXX"
    } else if rssi > -60 {
      "XXX "
    } else if rssi > -70 {
      "XX  "
    } else {
      "X   "
    }
  }

  async fn connect_to_network(&mut self, ssid: &str) {
    let known_networks = self.ctx.platform.config_manager().get_data().await.known_wifi_networks;
    let known = known_networks.iter().find(|n| n.ssid == ssid);
    match known {
      Some(_) => {
        info!("Connecting to known network: {}", ssid);
        self
          .ctx
          .platform
          .wifi_manager()
          .set_desired_state(crate::types::WifiDesiredState::Online)
          .await;
        self.ctx.notify(format!("Connecting to {}...", ssid), Icon40::Info).await;
      }
      None => {
        info!("No saved password for: {}", ssid);
        self.ctx.notify("No saved password", Icon40::Info).await;
      }
    }
  }
}

impl<P: Platform> MenuApp for WifiScannerApp<P> {
  fn render(&self) -> LcdScreen {
    match &self.state.screen {
      Screen::Scanning => LcdScreen::Progress("Scanning WiFi...".to_string()),
      Screen::NetworkList => {
        let menu: Vec<MenuLine> = self
          .state
          .networks
          .iter()
          .map(|net| MenuLine(Icon20::Wifi, format!("{} {}", Self::signal_bars(net.signal_strength), net.ssid)))
          .collect();
        LcdScreen::Menu {
          menu,
          selected: self.state.cursor as u32,
          animation: MenuAnimation::FromRight,
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
          Screen::NetworkList => match hex {
            HexButton::Up => {
              if self.state.cursor > 0 {
                self.state.cursor -= 1;
              }
            }
            HexButton::Down => {
              if self.state.cursor + 1 < self.state.networks.len() {
                self.state.cursor += 1;
              }
            }
            HexButton::Fire => {
              if let Some(name) = self.state.networks.get(self.state.cursor).map(|n| n.ssid.clone()) {
                self.connect_to_network(&name).await;
              }
            }
            HexButton::Right => {
              self.refresh_scan().await;
            }
            _ => {}
          },
        }
        AppAction::Continue
      }
    }
  }
}
