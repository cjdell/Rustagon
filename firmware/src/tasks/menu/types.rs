use crate::{apps::MenuAppInputChannel, tasks::lcd::LcdSignal, types::*, utils::*};
use alloc::string::{String, ToString};
use core::net::Ipv4Addr;
use embassy_net::Stack;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub enum Menu {
  Root,
  Information,
  Config,
  Files(String),
  Wifi,
}

impl ToString for Menu {
  fn to_string(&self) -> String {
    match self {
      Menu::Root => "Root".to_string(),
      Menu::Information => "Information".to_string(),
      Menu::Config => "Config".to_string(),
      Menu::Files(_) => "Files".to_string(),
      Menu::Wifi => "Wifi".to_string(),
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AppType {
  MenuApp,
  NativeApp,
}

#[derive(Clone)]
pub enum MenuOption {
  App {
    name: &'static str,
    app_type: AppType,
  },
  Stop,
  Setting {
    name: String,
    setting: Setting,
    setting_type: SettingType,
  },
  Menu {
    menu: Menu,
  },
  Item {
    name: String,
    item_type: ItemType,
  },
  Text {
    text: String,
  },
  Back,
  PowerOff,
}

#[derive(Debug, Clone)]
pub enum Setting {
  WifiToggle,
  WifiMode,
  Format,
}

#[derive(Debug, Clone)]
pub enum SettingType {
  Boolean,
}

#[derive(Debug, Clone)]
pub enum ItemType {
  File,
  Directory,
  WifiNetwork { rssi: i32 },
}

pub enum WifiStatus {
  Offline,
  Connected(Ipv4Addr),
  AccessPoint,
}

pub struct MenuRunnerContext {
  pub stack: Stack<'static>,
  pub local_fs: LocalFs,
  pub device_state: DeviceState,

  pub system_receiver: SystemReceiver,
  pub hex_button_subscriber: HexButtonReceiver,

  pub http_event_receiver: HttpReceiver,
  pub host_ipc_sender: HostIpcSender,
  pub wasm_ipc_channel: &'static WasmIpcChannel,

  pub lcd_signal: &'static LcdSignal,
  pub platform: crate::platform::HardwarePlatform,
}

#[derive(Clone)]
pub struct MenuContext {
  pub stack: Stack<'static>,
  pub local_fs: LocalFs,
  pub device_state: DeviceState,
  pub platform: crate::platform::HardwarePlatform,

  pub host_ipc_sender: HostIpcSender,
  pub lcd_signal: &'static LcdSignal,

  pub menu_app_input_channel: &'static MenuAppInputChannel,
}
