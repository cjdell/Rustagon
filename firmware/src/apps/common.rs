use crate::platform::{self, Platform};
use crate::types::{HexButton, HostIpcSender, LcdScreen};
use alloc::string::String;
use embassy_net::Stack;
use embassy_sync::{
  blocking_mutex::raw::NoopRawMutex,
  channel::{Channel, Receiver, Sender},
};

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

pub struct MenuAppContext {
  pub input_receiver: MenuAppInputReceiver,
  pub stack: Stack<'static>,
  pub platform: platform::HardwarePlatform,
  pub host_ipc_sender: HostIpcSender,
}

impl MenuAppContext {
  pub fn new(
    input_receiver: MenuAppInputReceiver,
    stack: Stack<'static>,
    platform: platform::HardwarePlatform,
    host_ipc_sender: HostIpcSender,
  ) -> Self {
    Self {
      input_receiver,
      stack,
      platform,
      host_ipc_sender,
    }
  }

  pub fn update_lcd(&self, screen: LcdScreen) {
    let _ = self.platform.display_manager().signal(screen);
  }
}
