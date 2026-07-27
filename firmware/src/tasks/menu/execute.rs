use crate::{
  apps::*,
  device::DeviceConfigurator as _,
  platform::{self, Platform},
  protocol::*,
  tasks::menu::{menus::MenuProvider as _, state::MenuState, types::*},
  types::*,
};
use alloc::string::ToString as _;
use embassy_time::{Duration, Timer};
use log::{info, warn};

impl MenuState {
  pub async fn execute_option(&mut self) -> () {
    let mut new_menu: Option<Menu> = None;

    match &self.menu_options[self.selected as usize] {
      MenuOption::App { name, app_type } => {
        info!("Open {name}");

        match app_type {
          AppType::MenuApp => {
            self.ctx.menu_app_input_channel.send(MenuAppInput::Start(name.to_string())).await;
          }
          AppType::NativeApp => {
            self.ctx.host_ipc_sender.send((0, HostIpcMessage::StartNative(name.to_string()))).await;
          }
        }
      }
      MenuOption::Stop => (),
      MenuOption::Setting {
        name,
        setting,
        setting_type,
      } => {
        info!("Change {name} {setting_type:?}");
        match setting {
          Setting::WifiToggle => {
            if let WifiStatus::Offline = self.wifi_status {
              info!("Menu: Online");
              self.ctx.platform.wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Online).await;
            } else {
              info!("Menu: Offline");
              self.ctx.platform.wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Offline).await;
            }
          }
          Setting::WifiMode => {
            let mut cfg = self.ctx.platform.config_manager().get_data().await;
            match cfg.wifi_mode {
              crate::types::WifiMode::Station => {
                cfg.wifi_mode = crate::types::WifiMode::AccessPoint;
                self.ctx.platform.config_manager().set_data(cfg).await;
                self.ctx.platform.config_manager().save().await.unwrap();
                esp_hal::system::software_reset();
              }
              crate::types::WifiMode::AccessPoint => {
                cfg.wifi_mode = crate::types::WifiMode::Station;
                self.ctx.platform.config_manager().set_data(cfg).await;
                self.ctx.platform.config_manager().save().await.unwrap();
                esp_hal::system::software_reset();
              }
            };
          }
          Setting::Format => {
            let _ = self.ctx.display.signal(LcdScreen::Headline(Icon40::Info, "Formatting...".to_string()));
            if let Err(err) = self.ctx.platform.storage_manager().format().await {
              warn!("Format Error: {err:?}");
              let _ = self.ctx.display.signal(LcdScreen::Headline(Icon40::Error, "Format Failed!".to_string()));
              Timer::after(Duration::from_secs(3)).await;
            } else {
              warn!("File System Formatted! Rebooting...");
              let _ = self.ctx.display.signal(LcdScreen::Headline(Icon40::Info, "Format Complete!".to_string()));
              Timer::after(Duration::from_secs(2)).await;
              esp_hal::system::software_reset();
            }
          }
        };
      }
      MenuOption::Menu { menu } => new_menu = Some(menu.clone()),
      MenuOption::Item { name, item_type } => {
        match item_type {
          ItemType::File => {
            info!("Open file: {}", name);
            self.ctx.host_ipc_sender.send((0, HostIpcMessage::StartWasm(name.clone()))).await;
          }
          ItemType::Directory => {
            info!("Enter directory: {}", name);
            // Could navigate to subdirectory
            new_menu = Some(Menu::Files(name.clone()));
          }
          ItemType::WifiNetwork { rssi } => {
            info!("Connect to WiFi: {} (signal: {})", name, rssi);
          }
        };
      }
      MenuOption::Text { text: _ } => {}
      MenuOption::Back => new_menu = Some(Menu::Root),
      MenuOption::PowerOff => {
        self.ctx.platform.power_manager().power_off().await;
      }
    };

    if let Some(new_menu) = new_menu {
      self.current_menu = new_menu;
      self.menu_options = self.get_menu_provider().await.get_items().await;
      self.selected = 0; // Reset selection when changing menus
    }
  }
}
