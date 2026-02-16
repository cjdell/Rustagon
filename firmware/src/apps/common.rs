use crate::{
  lib::{DeviceState, HexButton, LcdScreen},
  tasks::lcd::LcdSignal,
  utils::local_fs::LocalFs,
};
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
  pub local_fs: LocalFs,
  pub device_state: DeviceState,
  pub stack: Stack<'static>,
  lcd_signal: &'static LcdSignal,
}

impl MenuAppContext {
  pub fn new(
    input_receiver: MenuAppInputReceiver,
    local_fs: LocalFs,
    device_state: DeviceState,
    stack: Stack<'static>,
    lcd_signal: &'static LcdSignal,
  ) -> Self {
    Self {
      input_receiver,
      local_fs,
      device_state,
      stack,
      lcd_signal,
    }
  }

  pub fn update_lcd(&self, screen: LcdScreen) {
    self.lcd_signal.signal(screen);
  }
}
