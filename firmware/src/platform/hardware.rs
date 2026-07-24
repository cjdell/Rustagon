use super::led::{HardwareLedManager, LedHandle};
use super::power::PowerHandle;
use super::traits::Platform;
use super::wifi::WiFiHandle;
use alloc::sync::Arc;
use core::fmt;
use embassy_executor::Spawner;
use crate::utils::MaskedI2cBus;

/// Real hardware platform implementation - stores concrete LED manager and a reference to power handle
#[derive(Clone, Debug)]
pub struct HardwarePlatform {
  led: LedHandle,
  power: PowerHandle,
  wifi: WiFiHandle,
}

impl HardwarePlatform {
  /// Create a new hardware platform and initialize all hardware managers
  /// Note: This should be called with sys_bus that has been configured separately
  /// The power manager is passed in separately to avoid Sync issues with the I2C bus
  pub fn new_with_managers(led: LedHandle, power: PowerHandle, wifi: WiFiHandle) -> Self {
    Self { led, power, wifi }
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
}
