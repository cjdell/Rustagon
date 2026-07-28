use super::display::DisplayHandle;
use super::input::InputHandle;
use super::led::{HardwareLedManager, LedHandle};
use super::power::PowerHandle;
use super::storage::{ConfigHandle, FsError, HardwareStorageManager, StorageHandle};
use super::system::SystemHandle;
use super::traits::Platform;
use super::wifi::WiFiHandle;
use crate::utils::MaskedI2cBus;
use alloc::sync::Arc;
use core::fmt;
use embassy_executor::Spawner;

/// Real hardware platform implementation - stores concrete manager handles
#[derive(Clone, Debug)]
pub struct HardwarePlatform {
  display: DisplayHandle,
  led: LedHandle,
  power: PowerHandle,
  wifi: WiFiHandle,
  input: InputHandle,
  system: SystemHandle,
  storage: StorageHandle,
  config: ConfigHandle,
  storage_formatter: HardwareStorageManager,
}

impl HardwarePlatform {
  /// Create a new hardware platform with all manager handles
  pub fn new_with_managers(
    display: DisplayHandle,
    led: LedHandle,
    power: PowerHandle,
    wifi: WiFiHandle,
    input: InputHandle,
    system: SystemHandle,
    storage: StorageHandle,
    config: ConfigHandle,
    storage_formatter: HardwareStorageManager,
  ) -> Self {
    Self {
      display,
      led,
      power,
      wifi,
      input,
      system,
      storage,
      config,
      storage_formatter,
    }
  }
}

impl Platform for HardwarePlatform {
  fn display_manager(&self) -> DisplayHandle {
    self.display.clone()
  }

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

  fn system_manager(&self) -> SystemHandle {
    self.system.clone()
  }

  fn storage_manager(&self) -> StorageHandle {
    self.storage.clone()
  }

  fn config_manager(&self) -> ConfigHandle {
    self.config.clone()
  }

  async fn format_storage(&self) -> Result<(), FsError> {
    self.storage_formatter.format().await
  }
}
