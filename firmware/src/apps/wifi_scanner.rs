use crate::{
  apps::{
    MenuAppAsync, MenuAppInput,
    common::{AppName, MenuAppContext},
  },
  platform::Platform,
  types::*,
  utils::sleep,
};
use alloc::{format, string::String, string::ToString, vec, vec::Vec};
use log::info;

pub struct WifiScannerApp {
  ctx: MenuAppContext,
  state: AppState,
}

impl AppName for WifiScannerApp {
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
  networks: Vec<crate::platform::WifiResult>,
  cursor: usize,
  status_message: Option<String>,
}

impl AppState {
  fn new() -> Self {
    Self {
      screen: Screen::Scanning,
      networks: Vec::new(),
      cursor: 0,
      status_message: None,
    }
  }
}

impl WifiScannerApp {
  pub fn new(ctx: MenuAppContext) -> Self {
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  async fn set_status(&mut self, msg: String) {
    self.state.status_message = Some(msg);
    let screen = self.render();
    self.ctx.update_lcd(screen);
    sleep(2_000).await;
    self.state.status_message = None;
  }

  fn render(&self) -> LcdScreen {
    match &self.state.screen {
      Screen::Scanning => {
        LcdScreen::Progress("Scanning WiFi...".to_string())
      }
      Screen::NetworkList => {
        if let Some(msg) = &self.state.status_message {
          LcdScreen::Headline(Icon40::Info, msg.clone())
        } else {
          let menu: Vec<MenuLine> = self
            .state
            .networks
            .iter()
            .map(|net| {
              let signal = Self::signal_bars(net.signal_strength);
              MenuLine(Icon20::Wifi, format!("{} {}", signal, net.ssid))
            })
            .collect();

          LcdScreen::Menu {
            menu,
            selected: self.state.cursor as u32,
          }
        }
      }
    }
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
    let known_networks = self.ctx.config.get_data().await.known_wifi_networks;
    let known = known_networks.iter().find(|n| n.ssid == ssid);

    match known {
      Some(_network) => {
        info!("Connecting to known network: {}", ssid);
        self.ctx.platform().wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Online).await;
        self.set_status(format!("Connecting to {}...", ssid)).await;
      }
      None => {
        info!("No saved password for: {}", ssid);
        self.set_status("No saved password".to_string()).await;
      }
    }
  }
}

impl MenuAppAsync for WifiScannerApp {
  async fn work(&mut self) -> bool {
    // Initial scan
    self.state.screen = Screen::Scanning;
    self.ctx.update_lcd(self.render());

    let networks = self.ctx.platform().wifi_manager().scan().await;

    self.state.networks = networks;
    self.state.screen = Screen::NetworkList;

    loop {
      self.ctx.update_lcd(self.render());

      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => {
          match &self.state.screen {
            Screen::Scanning => {} // Ignore input while scanning
            Screen::NetworkList => {
              self.state.status_message = None;
              match input {
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
                  let ssid = self.state.networks.get(self.state.cursor).map(|n| n.ssid.clone());
                  if let Some(ref name) = ssid {
                    self.connect_to_network(name).await;
                  }
                }
                HexButton::Right => {
                  // Re-scan
                  self.state.screen = Screen::Scanning;
                  self.ctx.update_lcd(self.render());

                  let networks = self.ctx.platform().wifi_manager().scan().await;
                  self.state.networks = networks;
                  self.state.screen = Screen::NetworkList;
                  self.state.cursor = 0;
                }
                _ => {}
              }
            }
          }
        }
        MenuAppInput::Stop => {
          return false;
        }
        _ => {}
      }
    }
  }
}
