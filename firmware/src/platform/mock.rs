use super::input::{InputHandle, MockInputManager};
use super::led::{LedHandle, MockLedManager};
use super::power::{MockPowerManager, PowerHandle};
use super::traits::Platform;
use super::wifi::{MockWifiManager, WiFiHandle};
use alloc::sync::Arc;
use core::fmt;

/// Mock platform implementation for testing without hardware
#[derive(Clone, Debug)]
pub struct MockPlatform {
  led: LedHandle,
  power: PowerHandle,
  wifi: WiFiHandle,
  input: InputHandle,
}

impl MockPlatform {
  pub fn new() -> Self {
    let led_manager = Arc::new(MockLedManager::new());
    let led = LedHandle::new(led_manager);

    let power_manager = Arc::new(MockPowerManager::new());
    let power = PowerHandle::new(power_manager);

    let wifi_manager = MockWifiManager::new();
    let wifi = WiFiHandle::new(wifi_manager);

    let input_manager = MockInputManager::new();
    let input = InputHandle::new(input_manager);

    Self { led, power, wifi, input }
  }
}

impl Default for MockPlatform {
  fn default() -> Self {
    Self::new()
  }
}

impl Platform for MockPlatform {
  fn led_manager(&self) -> LedHandle {
    self.led.clone()
  }

  fn power_manager(&self) -> PowerHandle {
    self.power.clone()
  }

  fn wifi_manager(&self) -> WiFiHandle {
    self.wifi.clone()
  }

  fn input_manager(&self) -> InputHandle {
    self.input.clone()
  }
}
