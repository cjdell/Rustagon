use super::display::{DisplayHandle, MockDisplayManager};
use super::input::{InputHandle, MockInputManager};
use super::led::{LedHandle, MockLedManager};
use super::power::{MockPowerManager, PowerHandle};
use super::storage::{ConfigHandle, FsError, MockConfigManager, MockStorageManager, StorageHandle};
use super::traits::Platform;
use super::wifi::{MockWifiManager, WiFiHandle};
use alloc::sync::Arc;
use core::fmt;

/// Mock platform implementation for testing without hardware
#[derive(Clone, Debug)]
pub struct MockPlatform {
  display: DisplayHandle,
  led: LedHandle,
  power: PowerHandle,
  wifi: WiFiHandle,
  input: InputHandle,
  storage: StorageHandle,
  config: ConfigHandle,
}

impl MockPlatform {
  pub fn new() -> Self {
    let display_manager = Arc::new(MockDisplayManager::new());
    let display = DisplayHandle::new(display_manager);

    let led_manager = Arc::new(MockLedManager::new());
    let led = LedHandle::new(led_manager);

    let power_manager = Arc::new(MockPowerManager::new());
    let power = PowerHandle::new(power_manager);

    let wifi_manager = MockWifiManager::new();
    let wifi = WiFiHandle::new(wifi_manager);

    let input_manager = MockInputManager::new();
    let input = InputHandle::new(input_manager);

    let storage_manager = MockStorageManager::new();
    let storage = StorageHandle::new(Arc::new(storage_manager));

    let config = ConfigHandle::new(Arc::new(MockConfigManager::new()));

    Self { display, led, power, wifi, input, storage, config }
  }
}

impl Default for MockPlatform {
  fn default() -> Self {
    Self::new()
  }
}

impl Platform for MockPlatform {
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

  fn system_manager(&self) -> super::SystemHandle {
    todo!()
  }

  fn storage_manager(&self) -> StorageHandle {
    self.storage.clone()
  }

  fn config_manager(&self) -> ConfigHandle {
    self.config.clone()
  }

  async fn format_storage(&self) -> Result<(), FsError> {
    Ok(())
  }
}
