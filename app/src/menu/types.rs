use crate::apps::MenuAppContext;
use crate::menu::state::StackEventHandle;
use crate::platform::Platform;
use crate::protocol::HostIpcSender;
use alloc::{boxed::Box, string::String, vec::Vec};
use core::{fmt, future::Future, pin::Pin};

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
  App { name: &'static str, app_type: AppType },
  Back,
  PowerOff,
}

#[derive(Clone, Debug)]
pub enum AppType {
  MenuApp,
  NativeApp,
}

pub type AppLoader<P> = fn(String, MenuAppContext<P>) -> Pin<Box<dyn Future<Output = ()>>>;

pub struct MenuRunnerContext<P: Platform> {
  pub platform: P,
  pub host_ipc_sender: HostIpcSender,
  pub stack_event_handle: StackEventHandle,
  pub app_loader: Option<AppLoader<P>>,
  pub additional_apps: &'static [&'static str],
  /// WASM app bytes to launch immediately on startup instead of using the menu.
  pub auto_launch: Option<Vec<u8>>,
}

impl<P: Platform> Clone for MenuRunnerContext<P> {
  fn clone(&self) -> Self {
    Self {
      platform: self.platform.clone(),
      host_ipc_sender: self.host_ipc_sender,
      stack_event_handle: self.stack_event_handle.clone(),
      app_loader: self.app_loader,
      additional_apps: self.additional_apps,
      auto_launch: self.auto_launch.clone(),
    }
  }
}

impl<P: Platform> fmt::Debug for MenuRunnerContext<P> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MenuRunnerContext").finish()
  }
}
