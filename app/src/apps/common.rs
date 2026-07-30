use crate::platform::{display::DisplayHandle, Platform};
use crate::protocol::HostIpcSender;
use crate::types::{DeviceEvent, HexpansionEvent, HexButton};
use alloc::string::String;
use display_types::LcdScreen;

pub trait AppName {
  fn app_name() -> &'static str;
}

pub struct MenuAppContext<P: Platform> {
  pub platform: P,
  pub display: DisplayHandle,
  pub host_ipc_sender: HostIpcSender,
}

impl<P: Platform> MenuAppContext<P> {
  pub fn new(platform: P, host_ipc_sender: HostIpcSender) -> Self {
    let display = platform.display_manager();
    Self { platform, display, host_ipc_sender }
  }

  pub fn update_lcd(&self, screen: LcdScreen) {
    let _ = self.display.signal(screen);
  }
}

impl<P: Platform> Clone for MenuAppContext<P> {
  fn clone(&self) -> Self {
    Self {
      platform: self.platform.clone(),
      display: self.display.clone(),
      host_ipc_sender: self.host_ipc_sender,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
  Continue,
  Stop,
  LaunchWasm(String),
  LaunchNative(String),
}

pub trait MenuApp {
  fn render(&self) -> LcdScreen;
  async fn init(&mut self);
  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction;

  /// Called when external events occur (hexpansion plug/unplug, etc.) while
  /// the app is in the foreground. Default does nothing — override to react
  /// to state changes without requiring a button press.
  async fn handle_event(&mut self, _event: AppEvent) {}
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
  Hexpansion(HexpansionEvent),
  Device(DeviceEvent),
}

pub enum MenuAppInput {
  Button(HexButton),
  Stop,
}
