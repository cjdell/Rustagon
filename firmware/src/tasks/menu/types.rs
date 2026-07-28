use crate::{apps::MenuAppInputChannel, platform::{self, display::DisplayHandle, StorageHandle}, types::*, utils::*};
use alloc::string::String;
use embassy_net::Stack;

#[derive(Clone)]
pub enum Menu {
  Root,
  Files(String),
}

impl Menu {
  pub fn label(&self) -> &str {
    match self {
      Menu::Root => "Root",
      Menu::Files(_) => "Files",
    }
  }
}

#[derive(Clone)]
pub enum MenuOption {
  App {
    name: &'static str,
    app_type: AppType,
  },
  Menu {
    menu: Menu,
  },
  Item {
    name: String,
    item_type: ItemType,
  },
  Back,
  PowerOff,
}

#[derive(Clone, Debug)]
pub enum AppType {
  MenuApp,
  NativeApp,
}

#[derive(Debug, Clone)]
pub enum ItemType {
  File,
  Directory,
}

pub struct MenuRunnerContext {
  pub stack: Stack<'static>,
  pub storage: StorageHandle,

  pub http_event_receiver: HttpReceiver,
  pub host_ipc_sender: HostIpcSender,
  pub wasm_ipc_channel: &'static WasmIpcChannel,
  pub platform: platform::HardwarePlatform,
}

#[derive(Clone)]
pub struct MenuContext {
  pub stack: Stack<'static>,
  pub storage: StorageHandle,
  pub platform: platform::HardwarePlatform,

  pub host_ipc_sender: HostIpcSender,
  pub display: DisplayHandle,

  pub menu_app_input_channel: &'static MenuAppInputChannel,
}
