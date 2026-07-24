use super::led::{LedHandle, MockLedManager};
use super::power::{MockPowerManager, PowerHandle};
use super::traits::Platform;
use alloc::sync::Arc;
use core::fmt;

/// Mock platform implementation for testing without hardware
#[derive(Clone, Debug)]
pub struct MockPlatform {
  led: LedHandle,
  power: PowerHandle,
}

impl MockPlatform {
  pub fn new() -> Self {
    let led_manager = Arc::new(MockLedManager::new());
    let led = LedHandle::new(led_manager);

    let power_manager = Arc::new(MockPowerManager::new());
    let power = PowerHandle::new(power_manager);

    Self { led, power }
  }
}

impl Default for MockPlatform {
  fn default() -> Self {
    Self::new()
  }
}

impl Platform for MockPlatform {
  fn led(&self) -> LedHandle {
    self.led.clone()
  }

  fn power(&self) -> PowerHandle {
    self.power.clone()
  }
}
