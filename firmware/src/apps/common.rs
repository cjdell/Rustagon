use crate::platform::{display::DisplayHandle, ConfigHandle, StorageHandle};
use crate::types::{HexButton, LcdScreen};
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
  pub storage: StorageHandle,
  pub config: ConfigHandle,
  pub stack: Stack<'static>,
  display: DisplayHandle,
}

impl MenuAppContext {
  pub fn new(
    input_receiver: MenuAppInputReceiver,
    storage: StorageHandle,
    config: ConfigHandle,
    stack: Stack<'static>,
    display: DisplayHandle,
  ) -> Self {
    Self {
      input_receiver,
      storage,
      config,
      stack,
      display,
    }
  }

  pub fn update_lcd(&self, screen: LcdScreen) {
    let _ = self.display.signal(screen);
  }
}
