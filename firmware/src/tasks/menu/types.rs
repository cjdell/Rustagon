use crate::{apps::MenuAppInputChannel, platform::{self, display::DisplayHandle, StorageHandle}, types::*};
use alloc::sync::Arc;
use embassy_net::Stack;

#[derive(Clone)]
pub enum Menu {
  Root,
}

impl Menu {
  pub fn label(&self) -> &str {
    match self {
      Menu::Root => "Root",
    }
  }
}

#[derive(Clone)]
pub enum MenuOption {
  App {
    name: &'static str,
    app_type: AppType,
  },
  Back,
  PowerOff,
}

#[derive(Clone, Debug)]
pub enum AppType {
  MenuApp,
  NativeApp,
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
