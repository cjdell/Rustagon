use super::input::InputHandle;
use super::led::{HardwareLedManager, LedHandle};
use super::power::PowerHandle;
use super::traits::Platform;
use super::wifi::WiFiHandle;
use alloc::sync::Arc;
use core::fmt;
use crate::utils::MaskedI2cBus;
use embassy_executor::Spawner;

/// Real hardware platform implementation - stores concrete manager handles
#[derive(Clone, Debug)]
pub struct HardwarePlatform {
  led: LedHandle,
  power: PowerHandle,
  wifi: WiFiHandle,
  input: InputHandle,
}

impl HardwarePlatform {
  /// Create a new hardware platform with all manager handles
  pub fn new_with_managers(
    led: LedHandle,
    power: PowerHandle,
    wifi: WiFiHandle,
    input: InputHandle,
  ) -> Self {
    Self { led, power, wifi, input }
  }
}

impl Platform for HardwarePlatform {
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
