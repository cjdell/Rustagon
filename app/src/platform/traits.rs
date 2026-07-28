use super::display::DisplayHandle;
use super::input::InputHandle;
use super::led::LedHandle;
use super::power::PowerHandle;
use super::storage::{ConfigHandle, FsError, StorageHandle};
use super::system::SystemHandle;
use super::wifi::WiFiHandle;
use crate::types::DeviceConfig;
use core::fmt;

pub trait Platform: Clone + Send + Sync + fmt::Debug {
  fn display_manager(&self) -> DisplayHandle;
  fn led_manager(&self) -> LedHandle;
  fn power_manager(&self) -> PowerHandle;
  fn wifi_manager(&self) -> WiFiHandle;
  fn input_manager(&self) -> InputHandle;
  fn system_manager(&self) -> SystemHandle;
  fn storage_manager(&self) -> StorageHandle;
  fn config_manager(&self) -> ConfigHandle<DeviceConfig>;
  async fn format_storage(&self) -> Result<(), FsError>;
  async fn software_reset(&self);
}
