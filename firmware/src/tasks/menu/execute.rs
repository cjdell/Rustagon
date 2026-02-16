use crate::{
  apps::MenuAppInput,
  lib::{DeviceConfigurator as _, HostIpcMessage, Icon40, LcdScreen, PowerCtrl, WifiMode},
  tasks::{
    menu::{
      menus::MenuProvider as _,
      state::MenuState,
      types::{AppType, ItemType, Menu, MenuOption, Setting, WifiStatus},
    },
    wifi::{WifiCommandMessage, WifiDesiredState},
  },
  utils::local_fs::LocalFs,
};
use alloc::string::ToString as _;
use embassy_time::{Duration, Timer};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use log::{info, warn};
use picoserve::make_static;

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
              self.ctx.wifi_command_sender.send(WifiCommandMessage::ChangeState(WifiDesiredState::Online)).await;
            } else {
              info!("Menu: Offline");
              self.ctx.wifi_command_sender.send(WifiCommandMessage::ChangeState(WifiDesiredState::Offline)).await;
            }
          }
          Setting::WifiMode => {
            match self.ctx.device_state.get_wifi_mode() {
              WifiMode::Station => {
                self.ctx.device_state.set_wifi_mode(WifiMode::AccessPoint).unwrap();
                esp_hal::system::software_reset();
              }
              WifiMode::AccessPoint => {
                self.ctx.device_state.set_wifi_mode(WifiMode::Station).unwrap();
                esp_hal::system::software_reset();
              }
            };
          }
          Setting::Format => {
            self.ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Formatting...".to_string()));
            let flash = make_static!(FlashStorage, FlashStorage::new(unsafe { FLASH::steal() }));
            LocalFs::erase_filesystem(flash);
            warn!("File System Erased! Rebooting...");
            self.ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Erase Complete!".to_string()));
            Timer::after(Duration::from_secs(5)).await;
            esp_hal::system::software_reset();
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
        self.ctx.power_ctrl_sender.send(PowerCtrl::PowerOff).await;
      }
    };

    if let Some(new_menu) = new_menu {
      self.current_menu = new_menu;
      self.menu_options = self.get_menu_provider().get_items().await;
      self.selected = 0; // Reset selection when changing menus
    }
  }
}
