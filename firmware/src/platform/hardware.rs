use super::led::{HardwareLedManager, LedHandle};
use super::traits::Platform;
use alloc::sync::Arc;
use core::fmt;
use embassy_executor::Spawner;
use crate::utils::MaskedI2cBus;

/// Real hardware platform implementation
#[derive(Clone, Debug)]
pub struct HardwarePlatform {
  led: LedHandle,
}

impl HardwarePlatform {
  /// Create a new hardware platform and initialize all hardware managers
  pub fn new(spawner: &Spawner, sys_bus: MaskedI2cBus) -> Self {
    let led_manager = Arc::new(HardwareLedManager::new(spawner, sys_bus));
    let led = LedHandle::new(led_manager);
    Self { led }
  }
}

impl Platform for HardwarePlatform {
  fn led(&self) -> LedHandle {
    self.led.clone()
  }
}
