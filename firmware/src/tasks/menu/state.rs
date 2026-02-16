use crate::{
  apps::{MenuAppInput, MenuAppType},
  lib::{DeviceConfigurator, FIRMWARE_VERSION, HttpStatusMessage, Icon20, LcdScreen, MenuLine, WifiMode},
  native::NativeAppType,
  tasks::menu::{
    menus::{DynamicFilesystemMenu, DynamicWifiMenu, MenuTypes, StaticMenu},
    types::{AppType, ItemType, Menu, MenuContext, MenuOption, Setting, SettingType, WifiStatus},
  },
};
use alloc::vec;
use alloc::{borrow::ToOwned as _, boxed::Box, format, string::ToString as _, sync::Arc, vec::Vec};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, rwlock::RwLock};

pub struct MenuState {
  pub ctx: MenuContext,
  pub app: Arc<RwLock<NoopRawMutex, AppState>>,

  pub current_menu: Menu,
  pub menu_options: Vec<MenuOption>,
  pub selected: u32,

  pub wifi_status: WifiStatus,
  pub http_message: HttpStatusMessage,
}

pub enum AppState {
  None,
  MenuApp,
  HostedApp,
}

impl MenuState {
  pub async fn refresh(&mut self) {
    let app = &self.app.clone();
    if let Ok(app) = app.try_read() {
      match *app {
        AppState::None => {
          self.draw_menu().await;
        }
        AppState::MenuApp => {
          self.ctx.menu_app_input_channel.send(MenuAppInput::Refresh).await;
        }
        AppState::HostedApp => {
          // Hosted apps draw themselves
        }
      };
    }
  }

  pub fn get_menu_provider(&self) -> MenuTypes {
    match self.current_menu {
      Menu::Root => MenuTypes::StaticMenu(Box::new(StaticMenu {
        items: vec![
          MenuAppType::list_apps()
            .iter()
            .map(|name| MenuOption::App {
              name,
              app_type: AppType::MenuApp,
            })
            .collect(),
          NativeAppType::list_apps()
            .iter()
            .map(|name| MenuOption::App {
              name,
              app_type: AppType::NativeApp,
            })
            .collect(),
          vec![
            MenuOption::PowerOff,
            MenuOption::Menu {
              menu: Menu::Information,
            },
            MenuOption::Menu { menu: Menu::Config },
            MenuOption::Menu {
              menu: Menu::Files("/".to_string()),
            },
            MenuOption::Menu { menu: Menu::Wifi },
          ],
        ]
        .concat(),
      })),
      Menu::Information => MenuTypes::StaticMenu(Box::new({
        StaticMenu {
          items: vec![
            MenuOption::Text {
              text: match self.wifi_status {
                WifiStatus::Connected(ip) => format!("{ip:?}"),
                WifiStatus::AccessPoint => format!("AP:{}", self.ctx.device_state.get_data().ap_ssid),
                WifiStatus::Offline => format!("Disconnected"),
              },
            },
            MenuOption::Text {
              text: format!("FW ver: {}", FIRMWARE_VERSION),
            },
            MenuOption::Back,
          ],
        }
      })),
      Menu::Config => MenuTypes::StaticMenu(Box::new(StaticMenu {
        items: vec![
          MenuOption::Setting {
            name: if let WifiStatus::Connected(_) = self.wifi_status {
              "Disable Wifi"
            } else {
              "Enable Wifi"
            }
            .to_owned(),
            setting: Setting::WifiToggle,
            setting_type: SettingType::Boolean,
          },
          MenuOption::Setting {
            name: match self.ctx.device_state.get_wifi_mode() {
              WifiMode::Station => "Toggle AP Mode",
              WifiMode::AccessPoint => "Toggle STA Mode",
            }
            .to_owned(),
            setting: Setting::WifiMode,
            setting_type: SettingType::Boolean,
          },
          MenuOption::Setting {
            name: "Format FS".to_owned(),
            setting: Setting::Format,
            setting_type: SettingType::Boolean,
          },
          MenuOption::Back,
        ],
      })),
      Menu::Files(ref path) => MenuTypes::DynamicFilesystemMenu(Box::new(DynamicFilesystemMenu {
        local_fs: self.ctx.local_fs.clone(),
        path: path.clone(),
      })),
      Menu::Wifi => MenuTypes::DynamicWifiMenu(Box::new(DynamicWifiMenu::new(
        self.ctx.wifi_command_sender,
        self.ctx.wifi_scan_watch,
      ))),
    }
  }

  pub fn get_menu_screen(&self, menu: &Vec<MenuOption>) -> LcdScreen {
    match self.http_message {
      HttpStatusMessage::Progress(transferred, total) => {
        return LcdScreen::BoundedProgress(transferred, total);
      }
      _ => (),
    }

    LcdScreen::Menu {
      menu: menu
        .iter()
        .map(|option| match option {
          MenuOption::App { name, app_type: _ } => MenuLine(Icon20::Info, name.to_string()),
          MenuOption::Stop => MenuLine(Icon20::Info, "Stop".to_owned()),
          MenuOption::Setting {
            name,
            setting: _,
            setting_type: _,
          } => MenuLine(Icon20::Config, name.to_owned()),
          MenuOption::Menu { menu } => MenuLine(Icon20::Info, menu.to_string()),
          MenuOption::Item { name, item_type } => match item_type {
            ItemType::File => MenuLine(Icon20::File, format!("{}", name)),
            ItemType::Directory => MenuLine(Icon20::File, format!("{}", name)),
            ItemType::WifiNetwork { rssi } => {
              let signal = if *rssi > -50 {
                "XXXX"
              } else if *rssi > -60 {
                "XXX "
              } else if *rssi > -70 {
                "XX  "
              } else {
                "X   "
              };
              MenuLine(Icon20::Wifi, format!("{} {}", signal, name))
            }
          },
          MenuOption::Text { text } => MenuLine(Icon20::Info, text.clone()),
          MenuOption::Back => MenuLine(Icon20::Info, "<= Back".to_owned()),
          MenuOption::PowerOff => MenuLine(Icon20::Info, "Power Off".to_owned()),
        })
        .collect(),
      selected: self.selected,
    }
  }

  pub async fn draw_menu(&mut self) {
    // Ensure selected is within bounds
    if self.selected >= self.menu_options.len() as u32 {
      self.selected = if self.menu_options.is_empty() {
        0
      } else {
        self.menu_options.len() as u32 - 1
      };
    }

    self.ctx.lcd_signal.signal(self.get_menu_screen(&self.menu_options));
  }
}
