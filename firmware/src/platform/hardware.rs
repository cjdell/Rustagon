use super::display::DisplayHandle;
use super::input::InputHandle;
use super::led::{HardwareLedManager, LedHandle};
use super::power::PowerHandle;
use super::storage::{ConfigHandle, FsError, HardwareStorageManager, StorageHandle};
use super::system::SystemHandle;
use super::wifi::WiFiHandle;
use app::platform::Platform;
use app::types::OtaError;
use crate::utils::ota::Ota;
use alloc::sync::Arc;
use core::fmt;
use embassy_executor::Spawner;
use embedded_storage::Storage;
use procmacros::partition_offset;

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
    Self { display, led, power, wifi, input, system, storage, config, storage_formatter }
  }
}

const OTA_0_OFFSET: u32 = partition_offset!("ota_0");
const OTA_1_OFFSET: u32 = partition_offset!("ota_1");
const OTA_OFFSETS: [u32; 2] = [OTA_0_OFFSET, OTA_1_OFFSET];

impl Platform for HardwarePlatform {
  fn display_manager(&self) -> DisplayHandle { self.display.clone() }
  fn led_manager(&self) -> LedHandle { self.led.clone() }
  fn power_manager(&self) -> PowerHandle { self.power.clone() }
  fn wifi_manager(&self) -> WiFiHandle { self.wifi.clone() }
  fn input_manager(&self) -> InputHandle { self.input.clone() }
  fn system_manager(&self) -> SystemHandle { self.system.clone() }
  fn storage_manager(&self) -> StorageHandle { self.storage.clone() }
  fn config_manager(&self) -> ConfigHandle { self.config.clone() }

  async fn format_storage(&self) -> Result<(), FsError> {
    self.storage_formatter.format().await
  }

  async fn software_reset(&self) {
    esp_hal::system::software_reset();
  }

  async fn ota_begin(&self) -> Result<u32, OtaError> {
    let raw = self.storage_formatter.raw_flash();
    let mut flash = raw.write().await;

    let mut ota = Ota::new(&mut *flash);
    let slot = ota.current_slot().next();
    Ok(OTA_OFFSETS[slot.number()])
  }

  async fn ota_write_chunk(&self, offset: u32, data: &[u8]) -> Result<(), OtaError> {
    let raw = self.storage_formatter.raw_flash();
    let mut flash = raw.write().await;
    Storage::write(&mut *flash, offset, data).map_err(|_| OtaError::FlashWrite)
  }

  async fn ota_commit(&self) -> Result<(), OtaError> {
    let raw = self.storage_formatter.raw_flash();
    let mut flash = raw.write().await;

    let mut ota = Ota::new(&mut *flash);
    let slot = ota.current_slot().next();
    ota.set_current_slot(slot);
    Ok(())
  }
}
