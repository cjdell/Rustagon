use embedded_tools::local_fs::{FsError, LocalFs};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use esp32s3_embedded_tools::flash::LittleFsFlashStorage;
use esp_storage::FlashStorage as EspFlashStorage;
use alloc::sync::Arc;
use core::fmt;
use log::info;

/// Manages raw flash access for formatting the LittleFS filesystem.
///
/// Normal filesystem operations go through `LocalFs<LittleFsFlashStorage>`
/// directly (via `StorageHandle`), not through this type. This type only
/// exists to provide `format()` which requires creating a fresh storage
/// instance (can't format a mounted filesystem).
#[derive(Clone)]
pub struct HardwareStorageManager {
  raw_flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
  partition_offset: u32,
}

impl fmt::Debug for HardwareStorageManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareStorageManager").finish()
  }
}

impl HardwareStorageManager {
  pub fn new(
    raw_flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
    partition_offset: u32,
  ) -> Self {
    Self { raw_flash, partition_offset }
  }

  pub async fn format(&self) -> Result<(), FsError> {
    info!("Formatting filesystem...");
    let mut storage = LittleFsFlashStorage::new(self.raw_flash.clone(), self.partition_offset);
    LocalFs::format(&mut storage)
  }
}
