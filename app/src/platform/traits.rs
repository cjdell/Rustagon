use super::display::DisplayHandle;
use super::hexpansion::HexpansionHandle;
use super::http::HttpClientHandle;
use super::input::InputHandle;
use super::led::LedHandle;
use super::power::PowerHandle;
use super::storage::{ConfigHandle, FsError, StorageHandle};
use super::system::SystemHandle;
use super::tcp::TcpHandle;
use super::wifi::WiFiHandle;
use crate::types::{DeviceConfig, OtaError};
use core::fmt;

pub trait Platform: Clone + Send + Sync + fmt::Debug {
  fn display_manager(&self) -> DisplayHandle;
  fn led_manager(&self) -> LedHandle;
  fn power_manager(&self) -> PowerHandle;
  fn wifi_manager(&self) -> WiFiHandle;
  fn input_manager(&self) -> InputHandle;
  fn system_manager(&self) -> SystemHandle;
  fn hexpansion_manager(&self) -> HexpansionHandle;
  fn http_client(&self) -> Option<HttpClientHandle>;
  /// A raw TCP stream client (used by the SSH app). `None` on platforms that
  /// cannot open raw sockets.
  fn tcp_client(&self) -> Option<TcpHandle>;
  fn storage_manager(&self) -> StorageHandle;
  fn config_manager(&self) -> ConfigHandle<DeviceConfig>;
  /// The currently running firmware version, baked in at build time.
  fn firmware_version(&self) -> u32;
  /// Fill `dest` with cryptographically-secure random bytes from the device
  /// entropy source (hardware TRNG on firmware, OS RNG on desktop).
  fn entropy(&self, dest: &mut [u8]);
  async fn format_storage(&self) -> Result<(), FsError>;
  async fn software_reset(&self);

  /// Begin an OTA update. Returns the starting flash offset to write to.
  async fn ota_begin(&self) -> Result<u32, OtaError>;
  /// Write a chunk of firmware data at a flash offset.
  async fn ota_write_chunk(&self, offset: u32, data: &[u8]) -> Result<(), OtaError>;
  /// Finalise the OTA update and mark the new slot as bootable.
  async fn ota_commit(&self) -> Result<(), OtaError>;
}
