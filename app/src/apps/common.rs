use crate::platform::Platform;
use crate::protocol::HostIpcSender;
use crate::types::HexButton;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use display_types::LcdScreen;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::{Channel, Receiver, Sender}};

/// Set to true just before sending StartWasm/StartWasmWithBuffer.
/// The menu runner checks this flag when processing Stopped — if true,
/// it transitions to HostedApp instead of None, preventing the menu
/// from drawing over the WASM app's screen.
pub static WASM_LAUNCHING: AtomicBool = AtomicBool::new(false);

pub trait AppName {
  fn app_name() -> &'static str;
}

pub trait MenuAppAsync {
  async fn work(&mut self) -> bool;
}

#[derive(Debug, Clone)]
pub enum MenuAppInput {
  Start(String),
  Stop,
  Refresh,
  HexButton(HexButton),
}

pub type MenuAppInputChannel = Channel<NoopRawMutex, MenuAppInput, 1>;
pub type MenuAppInputReceiver = Receiver<'static, NoopRawMutex, MenuAppInput, 1>;
pub type MenuAppInputSender = Sender<'static, NoopRawMutex, MenuAppInput, 1>;

pub fn create_menu_app_channel() -> &'static MenuAppInputChannel {
  alloc::boxed::Box::leak(alloc::boxed::Box::new(MenuAppInputChannel::new()))
}

pub struct MenuAppContext<P: Platform> {
  pub input_receiver: MenuAppInputReceiver,
  pub platform: P,
  pub host_ipc_sender: HostIpcSender,
}

impl<P: Platform> MenuAppContext<P> {
  pub fn new(input_receiver: MenuAppInputReceiver, platform: P, host_ipc_sender: HostIpcSender) -> Self {
    Self { input_receiver, platform, host_ipc_sender }
  }

  pub fn update_lcd(&self, screen: LcdScreen) {
    let _ = self.platform.display_manager().signal(screen);
  }
}
