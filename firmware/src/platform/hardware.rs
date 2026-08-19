use super::display::DisplayHandle;
use super::input::InputHandle;
use super::led::LedHandle;
use super::power::PowerHandle;
use super::storage::{ConfigHandle, FsError, HardwareStorageManager, StorageHandle};
use super::system::SystemHandle;
use super::wifi::WiFiHandle;
use crate::utils::ota::Ota;
use app::platform::{HexpansionHandle, HttpClientHandle, Platform, SpawnerHandle, TcpHandle};
use app::types::OtaError;
use embedded_storage::Storage;
use procmacros::partition_offset;

/// Real hardware platform implementation - stores concrete manager handles
#[derive(Clone, Debug)]
pub struct HardwarePlatform {
  display: DisplayHandle,
  hexpansion: HexpansionHandle,
  led: LedHandle,
  power: PowerHandle,
  wifi: WiFiHandle,
  input: InputHandle,
  system: SystemHandle,
  storage: StorageHandle,
  config: ConfigHandle,
  storage_formatter: HardwareStorageManager,
  http_client: Option<HttpClientHandle>,
  tcp_client: Option<TcpHandle>,
  spawner: SpawnerHandle,
}

impl HardwarePlatform {
  // One parameter per manager handle; they mirror the Platform trait surface.
  #[allow(clippy::too_many_arguments)]
  pub fn new_with_managers(
    display: DisplayHandle,
    hexpansion: HexpansionHandle,
    led: LedHandle,
    power: PowerHandle,
    wifi: WiFiHandle,
    input: InputHandle,
    system: SystemHandle,
    storage: StorageHandle,
    config: ConfigHandle,
    storage_formatter: HardwareStorageManager,
    spawner: SpawnerHandle,
  ) -> Self {
    Self {
      display,
      hexpansion,
      led,
      power,
      wifi,
      input,
      system,
      storage,
      config,
      storage_formatter,
      http_client: None,
      tcp_client: None,
      spawner,
    }
  }

  pub fn with_http_client(mut self, client: HttpClientHandle) -> Self {
    self.http_client = Some(client);
    self
  }

  pub fn with_tcp_client(mut self, client: TcpHandle) -> Self {
    self.tcp_client = Some(client);
    self
  }
}

const OTA_0_OFFSET: u32 = partition_offset!("ota_0");
const OTA_1_OFFSET: u32 = partition_offset!("ota_1");
const OTA_OFFSETS: [u32; 2] = [OTA_0_OFFSET, OTA_1_OFFSET];

impl Platform for HardwarePlatform {
  fn display_manager(&self) -> DisplayHandle {
    self.display.clone()
  }
  fn hexpansion_manager(&self) -> HexpansionHandle {
    self.hexpansion.clone()
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
  fn http_client(&self) -> Option<HttpClientHandle> {
    self.http_client.clone()
  }
  fn tcp_client(&self) -> Option<TcpHandle> {
    self.tcp_client.clone()
  }
  fn storage_manager(&self) -> StorageHandle {
    self.storage.clone()
  }
  fn config_manager(&self) -> ConfigHandle {
    self.config.clone()
  }

  fn firmware_version(&self) -> u32 {
    crate::FIRMWARE_VERSION.parse().unwrap_or(0)
  }

  fn entropy(&self, dest: &mut [u8]) {
    let rng = esp_hal::rng::Rng::new();
    for chunk in dest.chunks_mut(4) {
      let v = rng.random().to_le_bytes();
      chunk.copy_from_slice(&v[..chunk.len()]);
    }
  }

  fn spawner(&self) -> SpawnerHandle {
    self.spawner.clone()
  }

  async fn format_storage(&self) -> Result<(), FsError> {
    self.storage_formatter.format().await
  }

  async fn software_reset(&self) {
    esp_hal::system::software_reset();
  }

  async fn ota_begin(&self) -> Result<u32, OtaError> {
    // No manual CPU parking: the flash storage is created with
    // `multicore_auto_park()` (see `bin/rustagon.rs`), which parks the app core
    // around every flash write/erase and unparks it on completion, including
    // on error. That strategy is the single owner of core parking for flash I/O.
    let raw = self.storage_formatter.raw_flash();
    let mut flash = raw.write().await;

    let mut ota = Ota::new(&mut flash);
    let slot = ota.target_slot();
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

    let mut ota = Ota::new(&mut flash);
    // Recomputing the target slot yields the same value as at `ota_begin`: the
    // only mutation of otadata in this session is the `set_current_slot` call
    // below. The Platform methods take `&self` (HardwarePlatform is Clone and
    // widely cloned), so there is no safe place to carry the slot between them.
    let slot = ota.target_slot();
    ota.set_current_slot(slot);
    Ok(())
  }
}
