use crate::apps::MenuAppContext;
use crate::menu::state::AppState;
use crate::platform::{display::DisplayHandle, Platform, StorageHandle};
use crate::protocol::HostIpcSender;
use alloc::{boxed::Box, string::String, sync::Arc};
use core::{fmt, future::Future, pin::Pin};
use embassy_net::Stack;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};

/// `Stack<'static>` is `!Send + !Sync` (uses `RefCell` internally), but in
/// practice it is only used from a single async executor core. This wrapper
/// makes it `Send + Sync` — same pattern as `SendFilesystem` in AGENTS.md.
#[derive(Clone, Copy)]
pub struct SendStack(pub Stack<'static>);

unsafe impl Send for SendStack {}
unsafe impl Sync for SendStack {}

impl fmt::Debug for SendStack {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("SendStack").finish()
  }
}

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

/// Custom loader for firmware-specific apps. The loader receives the
/// `MenuAppContext` after the generic `MenuAppType` loader failed to find
/// the app, and can call `.with_stack()` before constructing the app.
pub type AppLoader<P> =
  fn(String, MenuAppContext<P>) -> Pin<Box<dyn Future<Output = ()>>>;

pub struct MenuRunnerContext<P: Platform> {
  pub storage: StorageHandle,
  pub platform: P,
  pub host_ipc_sender: HostIpcSender,
  pub app_state: Option<Arc<RwLock<CriticalSectionRawMutex, AppState>>>,
  pub app_loader: Option<AppLoader<P>>,
  /// Additional app names to show in the menu (e.g., firmware-specific apps).
  /// These are tried via `app_loader` when the generic `MenuAppType` loader fails.
  pub additional_apps: &'static [&'static str],
  /// Network stack for apps that need streaming HTTP (AppStore, OTA).
  /// Wrapped in `SendStack` because `Stack<'static>` is `!Send + !Sync`.
  pub network_stack: Option<SendStack>,
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
      network_stack: self.network_stack.clone(),
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
