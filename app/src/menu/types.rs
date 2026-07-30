use crate::apps::MenuAppContext;
use crate::menu::state::AppState;
use crate::platform::{display::DisplayHandle, Platform, StorageHandle};
use crate::protocol::HostIpcSender;
use alloc::{boxed::Box, string::String, sync::Arc};
use core::{fmt, future::Future, pin::Pin};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};

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

/// Custom loader for firmware-specific apps not found in `MenuAppType`.
pub type AppLoader<P> =
  fn(String, MenuAppContext<P>) -> Pin<Box<dyn Future<Output = ()>>>;

pub struct MenuRunnerContext<P: Platform> {
  pub storage: StorageHandle,
  pub platform: P,
  pub host_ipc_sender: HostIpcSender,
  pub app_state: Option<Arc<RwLock<CriticalSectionRawMutex, AppState>>>,
  pub app_loader: Option<AppLoader<P>>,
  pub additional_apps: &'static [&'static str],
}

impl<P: Platform> Clone for MenuRunnerContext<P> {
  fn clone(&self) -> Self {
    Self {
      storage: self.storage.clone(),
      platform: self.platform.clone(),
      host_ipc_sender: self.host_ipc_sender,
      app_state: self.app_state.clone(),
      app_loader: self.app_loader,
      additional_apps: self.additional_apps,
    }
  }
}

pub struct MenuContext<P: Platform> {
  pub storage: StorageHandle,
  pub platform: P,
  pub host_ipc_sender: HostIpcSender,
  pub display: DisplayHandle,
  pub menu_app_input_channel: &'static crate::apps::MenuAppInputChannel,
  pub additional_apps: &'static [&'static str],
}

impl<P: Platform> Clone for MenuContext<P> {
  fn clone(&self) -> Self {
    Self {
      storage: self.storage.clone(),
      platform: self.platform.clone(),
      host_ipc_sender: self.host_ipc_sender,
      display: self.display.clone(),
      menu_app_input_channel: self.menu_app_input_channel,
      additional_apps: self.additional_apps,
    }
  }
}

impl<P: Platform> fmt::Debug for MenuRunnerContext<P> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MenuRunnerContext").finish()
  }
}
