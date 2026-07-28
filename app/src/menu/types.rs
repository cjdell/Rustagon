use crate::platform::{display::DisplayHandle, Platform, StorageHandle};
use crate::protocol::HostIpcSender;
use core::fmt;

#[derive(Clone)]
pub enum Menu { Root }

impl Menu {
  pub fn label(&self) -> &str { match self { Menu::Root => "Root" } }
}

#[derive(Clone)]
pub enum MenuOption {
  App { name: &'static str, app_type: AppType },
  Back,
  PowerOff,
}

#[derive(Clone, Debug)]
pub enum AppType { MenuApp, NativeApp }

pub struct MenuRunnerContext<P: Platform> {
  pub storage: StorageHandle,
  pub platform: P,
  pub host_ipc_sender: HostIpcSender,
}

impl<P: Platform> Clone for MenuRunnerContext<P> {
  fn clone(&self) -> Self {
    Self { storage: self.storage.clone(), platform: self.platform.clone(), host_ipc_sender: self.host_ipc_sender }
  }
}

impl<P: Platform> fmt::Debug for MenuRunnerContext<P> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MenuRunnerContext").finish()
  }
}

pub struct MenuContext<P: Platform> {
  pub storage: StorageHandle,
  pub platform: P,
  pub host_ipc_sender: HostIpcSender,
  pub display: DisplayHandle,
  pub menu_app_input_channel: &'static crate::apps::MenuAppInputChannel,
}

impl<P: Platform> Clone for MenuContext<P> {
  fn clone(&self) -> Self {
    Self {
      storage: self.storage.clone(),
      platform: self.platform.clone(),
      host_ipc_sender: self.host_ipc_sender,
      display: self.display.clone(),
      menu_app_input_channel: self.menu_app_input_channel,
    }
  }
}
